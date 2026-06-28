//! Parallel block scheduler for CMP2 JSONL streams.
//!
//! v0.8 starts by parallelizing the expensive per-block encode step while
//! preserving the existing CMP2 wire format. The scheduler reads JSONL in input
//! order, assigns each bounded row group a monotonically increasing block index,
//! encodes row groups on worker threads, and writes completed blocks only when
//! every earlier block is ready. That keeps output deterministic even when
//! workers finish out of order.

use std::collections::BTreeMap;
use std::future::Future;
use std::io::{BufRead, Read, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::io::{decode_jsonl, encode_jsonl_row_group};
use crate::limits::{MAX_PARALLEL_WORKERS, MAX_STREAM_BLOCKS};
use crate::primitives::crc32;
use crate::schema::Schema;
use crate::streaming::reader::{
    STREAM_HEADER_LEN, StreamRecord, count_jsonl_rows, ensure_reader_eof, parse_block_payload,
    read_next_record_from, usize_to_u64 as reader_usize_to_u64, validate_stream_header,
};
use crate::streaming::writer::{
    FILE_HEADER_LEN, encode_block_payload, normalize_jsonl_line, write_index_footer,
    write_stream_header,
};
use crate::streaming::{BlockMetadata, BlockOptions, read_bounded_jsonl_line};
use crate::{Codec, CompactError, Result, framing};

/// Configuration for the v0.8 parallel block encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelOptions {
    /// Number of worker threads used for block encode jobs.
    pub worker_count: usize,
    /// Existing CMP2 row-group limits. Every parallel job is one CMP2 block.
    pub block_options: BlockOptions,
}

/// Configuration for the v0.8 parallel block decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelDecodeOptions {
    /// Number of worker threads used for block decode jobs.
    pub worker_count: usize,
}

/// Runtime-agnostic async JSONL sink used by the v0.8 parallel decoder.
///
/// Core does not depend on Tokio or async-std. Runtime-specific crates can
/// implement this trait for their writer types and choose how the returned
/// future performs backpressure-aware writes.
pub trait AsyncJsonlSink {
    fn write_all<'a>(
        &'a mut self,
        bytes: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

impl AsyncJsonlSink for Vec<u8> {
    fn write_all<'a>(
        &'a mut self,
        bytes: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.extend_from_slice(bytes);
            Ok(())
        })
    }
}

impl ParallelDecodeOptions {
    /// Validate user-provided decode options before output is written.
    pub fn validate(self) -> Result<Self> {
        if self.worker_count == 0 {
            return Err(CompactError::InvalidInput(
                "parallel worker count must be greater than zero",
            ));
        }

        if self.worker_count > MAX_PARALLEL_WORKERS {
            return Err(CompactError::InvalidInput(
                "parallel worker count exceeds configured limit",
            ));
        }
        Ok(self)
    }
}

impl Default for ParallelDecodeOptions {
    fn default() -> Self {
        let worker_count = thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);

        Self { worker_count }
    }
}

impl ParallelOptions {
    /// Validate user-provided options before any output is written.
    pub fn validate(self) -> Result<Self> {
        if self.worker_count == 0 {
            return Err(CompactError::InvalidInput(
                "parallel worker count must be greater than zero",
            ));
        }

        if self.worker_count > MAX_PARALLEL_WORKERS {
            return Err(CompactError::InvalidInput(
                "parallel worker count exceeds configured limit",
            ));
        }
        Ok(Self {
            worker_count: self.worker_count,
            block_options: self.block_options.validate()?,
        })
    }
}

impl Default for ParallelOptions {
    fn default() -> Self {
        let worker_count = thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);

        Self {
            worker_count,
            block_options: BlockOptions::default(),
        }
    }
}

/// Encode JSONL into a CMP2 stream using multiple worker threads.
///
/// The output is byte-for-byte compatible with the sequential CMP2 reader and
/// inspector. Only block scheduling changes: input collection stays ordered,
/// block encode runs in parallel, and the collector writes frames plus the
/// `IDX1` footer in logical block order.
pub fn encode_jsonl_stream_parallel<R: BufRead + Send, W: Write>(
    mut input: R,
    mut output: W,
    schema: Schema,
    options: ParallelOptions,
) -> Result<W> {
    let options = options.validate()?;
    write_stream_header(&mut output)?;

    let channel_capacity =
        options
            .worker_count
            .checked_mul(2)
            .ok_or(CompactError::InvalidInput(
                "parallel channel capacity overflow",
            ))?;
    let (job_tx, job_rx) = mpsc::sync_channel(channel_capacity);
    let (result_tx, result_rx) = mpsc::sync_channel(channel_capacity);
    let job_rx = Arc::new(Mutex::new(job_rx));

    thread::scope(|scope| -> Result<()> {
        let mut workers = Vec::with_capacity(options.worker_count);
        for _ in 0..options.worker_count {
            let schema = schema.clone();
            let job_rx = Arc::clone(&job_rx);
            let result_tx = result_tx.clone();

            workers.push(
                thread::Builder::new()
                    .name("compact-encode-worker".to_owned())
                    .spawn_scoped(scope, move || worker_loop(schema, job_rx, result_tx))
                    .map_err(CompactError::Io)?,
            );
        }
        drop(job_rx);
        drop(result_tx);

        let scheduler = thread::Builder::new()
            .name("compact-encode-scheduler".to_owned())
            .spawn_scoped(scope, move || {
                schedule_jobs(&mut input, options.block_options, job_tx)
            })
            .map_err(CompactError::Io)?;
        let collected = collect_results(result_rx, &mut output);
        let job_count = join_scoped(scheduler)?;
        join_workers(workers)?;
        let (result_count, metadata) = collected?;
        if result_count != job_count {
            return Err(CompactError::InvalidInput(
                "parallel block results are incomplete",
            ));
        }
        write_index_footer(&mut output, &metadata)
    })?;

    Ok(output)
}

/// Decode a CMP2 JSONL stream using multiple worker threads.
///
/// The scanner reads frames sequentially and validates frame checksums plus
/// block metadata before work is dispatched. Worker threads decode column-block
/// payloads independently, and the collector writes JSONL in block order.
pub fn decode_jsonl_stream_parallel<R: Read + Send, W: Write>(
    mut input: R,
    mut output: W,
    schema: Schema,
    options: ParallelDecodeOptions,
) -> Result<W> {
    let options = options.validate()?;
    validate_stream_header(&mut input)?;

    let channel_capacity =
        options
            .worker_count
            .checked_mul(2)
            .ok_or(CompactError::InvalidInput(
                "parallel channel capacity overflow",
            ))?;
    let (job_tx, job_rx) = mpsc::sync_channel(channel_capacity);
    let (result_tx, result_rx) = mpsc::sync_channel(channel_capacity);
    let job_rx = Arc::new(Mutex::new(job_rx));

    thread::scope(|scope| -> Result<()> {
        let mut workers = Vec::with_capacity(options.worker_count);
        for _ in 0..options.worker_count {
            let schema = schema.clone();
            let job_rx = Arc::clone(&job_rx);
            let result_tx = result_tx.clone();

            workers.push(
                thread::Builder::new()
                    .name("compact-decode-worker".to_owned())
                    .spawn_scoped(scope, move || decode_worker_loop(schema, job_rx, result_tx))
                    .map_err(CompactError::Io)?,
            );
        }
        drop(job_rx);
        drop(result_tx);

        let scheduler = thread::Builder::new()
            .name("compact-decode-scheduler".to_owned())
            .spawn_scoped(scope, move || schedule_decode_jobs(&mut input, job_tx))
            .map_err(CompactError::Io)?;
        let collected = collect_decode_results(result_rx, &mut output);
        let job_count = join_scoped(scheduler)?;
        join_workers(workers)?;
        let result_count = collected?;
        if result_count != job_count {
            return Err(CompactError::InvalidInput(
                "parallel block results are incomplete",
            ));
        }
        Ok(())
    })?;

    Ok(output)
}

/// Decode a CMP2 JSONL stream and write the ordered result to an async sink.
///
/// This keeps v0.8's async writer contract independent from a specific runtime.
/// Each independently bounded block is decoded and awaited before the next one,
/// so sink backpressure bounds memory and cancellation stops further reads.
pub async fn decode_jsonl_stream_parallel_async_writer<R: Read + Send, W: AsyncJsonlSink>(
    mut input: R,
    mut output: W,
    schema: Schema,
    options: ParallelDecodeOptions,
) -> Result<W> {
    let options = options.validate()?;
    validate_stream_header(&mut input)?;
    let mut next_offset = STREAM_HEADER_LEN as u64;
    let mut expected_block_index = 0u64;
    let mut expected_first_row_index = 0u64;
    let mut metadata = Vec::new();
    let mut finished = false;

    while !finished {
        let mut jobs = Vec::with_capacity(options.worker_count);
        while jobs.len() < options.worker_count && !finished {
            let frame = match read_next_record_from(&mut input)? {
                StreamRecord::Frame(frame) => frame,
                StreamRecord::Index(index) => {
                    if index != metadata {
                        return Err(CompactError::InvalidInput(
                            "footer index does not match scanned blocks",
                        ));
                    }
                    ensure_reader_eof(&mut input)?;
                    finished = true;
                    continue;
                }
                StreamRecord::Eof => {
                    return Err(CompactError::InvalidInput(
                        "sealed stream footer is missing",
                    ));
                }
            };
            let frame_len = frame.len();
            let decoded = framing::decode_v1(&frame)?;
            if decoded.codec != Codec::ColumnBlock {
                return Err(CompactError::InvalidInput(
                    "stream block frame must use column block codec",
                ));
            }
            let parsed = parse_block_payload(&decoded.payload)?;
            if parsed.block_index != expected_block_index
                || parsed.first_row_index != expected_first_row_index
            {
                return Err(CompactError::InvalidInput(
                    "stream block metadata is not sequential",
                ));
            }
            if metadata.len() >= MAX_STREAM_BLOCKS {
                return Err(CompactError::InvalidInput(
                    "stream block count exceeds configured limit",
                ));
            }

            let block_metadata = BlockMetadata {
                block_index: parsed.block_index,
                encoded_offset: next_offset,
                row_count: reader_usize_to_u64(parsed.row_count, "row count is too large")?,
                uncompressed_size: reader_usize_to_u64(parsed.raw_size, "row group is too large")?,
                compressed_size: reader_usize_to_u64(frame_len, "block frame is too large")?,
                checksum: crc32::checksum(&decoded.payload),
            };
            jobs.push(DecodeJob {
                block_index: parsed.block_index,
                first_row_index: parsed.first_row_index,
                row_count: parsed.row_count,
                raw_size: parsed.raw_size,
                column_block: parsed.column_block.to_vec(),
            });
            expected_block_index = expected_block_index
                .checked_add(1)
                .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
            expected_first_row_index = expected_first_row_index
                .checked_add(block_metadata.row_count)
                .ok_or(CompactError::InvalidInput("parallel row count overflow"))?;
            next_offset = next_offset
                .checked_add(block_metadata.compressed_size)
                .ok_or(CompactError::InvalidInput(
                    "parallel stream offset overflow",
                ))?;
            metadata.push(block_metadata);
        }

        for decoded in decode_batch(&schema, jobs)? {
            output.write_all(decoded.jsonl.as_bytes()).await?;
        }
    }

    Ok(output)
}

fn decode_batch(schema: &Schema, jobs: Vec<DecodeJob>) -> Result<Vec<DecodedBlockJob>> {
    let mut decoded = thread::scope(|scope| -> Result<Vec<DecodedBlockJob>> {
        let mut workers = Vec::with_capacity(jobs.len());
        for job in jobs {
            workers.push(
                thread::Builder::new()
                    .name("compact-async-decode-worker".to_owned())
                    .spawn_scoped(scope, move || decode_job(schema, job))
                    .map_err(CompactError::Io)?,
            );
        }

        let mut decoded = Vec::with_capacity(workers.len());
        let mut first_error = None;
        for worker in workers {
            match join_scoped(worker) {
                Ok(block) => decoded.push(block),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(decoded),
        }
    })?;
    decoded.sort_unstable_by_key(|block| block.block_index);
    Ok(decoded)
}

fn worker_loop(
    schema: Schema,
    job_rx: Arc<Mutex<mpsc::Receiver<RowGroupJob>>>,
    result_tx: mpsc::SyncSender<Result<EncodedBlock>>,
) {
    loop {
        let job = match job_rx.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => {
                let _ = result_tx.send(Err(CompactError::InvalidInput(
                    "parallel worker pool stopped",
                )));
                break;
            }
        };

        let Ok(job) = job else {
            break;
        };

        if result_tx.send(encode_job(&schema, job)).is_err() {
            break;
        }
    }
}

fn schedule_jobs<R: BufRead>(
    input: &mut R,
    options: BlockOptions,
    job_tx: mpsc::SyncSender<RowGroupJob>,
) -> Result<u64> {
    let mut current = PendingRowGroup::default();
    let mut next_block_index = 0u64;
    let mut next_row_index = 0u64;

    while let Some(line) = read_bounded_jsonl_line(input, options.max_uncompressed_bytes_per_block)?
    {
        if line.trim().is_empty() {
            continue;
        }

        let line = normalize_jsonl_line(&line)?;
        if line.len() > options.max_uncompressed_bytes_per_block {
            return Err(CompactError::InvalidInput(
                "jsonl row exceeds max uncompressed bytes per block",
            ));
        }

        if current.would_exceed(&line, options) {
            let (job, row_count) = current.take_job(next_block_index, next_row_index);
            send_job(&job_tx, job)?;
            next_block_index = next_block_index
                .checked_add(1)
                .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
            next_row_index = next_row_index
                .checked_add(row_count)
                .ok_or(CompactError::InvalidInput("parallel row count overflow"))?;
        }

        current.push_line(&line);

        if current.reached_limit(options) {
            let (job, row_count) = current.take_job(next_block_index, next_row_index);
            send_job(&job_tx, job)?;
            next_block_index = next_block_index
                .checked_add(1)
                .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
            next_row_index = next_row_index
                .checked_add(row_count)
                .ok_or(CompactError::InvalidInput("parallel row count overflow"))?;
        }
    }

    if !current.is_empty() {
        let (job, _) = current.take_job(next_block_index, next_row_index);
        send_job(&job_tx, job)?;
        next_block_index = next_block_index
            .checked_add(1)
            .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
    }

    drop(job_tx);

    Ok(next_block_index)
}

fn send_job(job_tx: &mpsc::SyncSender<RowGroupJob>, job: RowGroupJob) -> Result<()> {
    job_tx
        .send(job)
        .map_err(|_| CompactError::InvalidInput("parallel worker pool stopped"))
}

fn encode_job(schema: &Schema, job: RowGroupJob) -> Result<EncodedBlock> {
    let row_group = encode_jsonl_row_group(&job.jsonl, schema)?;
    let block_payload = encode_block_payload(
        job.block_index,
        job.first_row_index,
        row_group.row_count,
        row_group.raw_bytes,
        &row_group.payload,
    )?;
    let encoded_frame = framing::encode_v1(Codec::ColumnBlock, &block_payload);
    let compressed_size = usize_to_u64(encoded_frame.len(), "block frame is too large")?;

    Ok(EncodedBlock {
        block_index: job.block_index,
        row_count: usize_to_u64(row_group.row_count, "row count is too large")?,
        uncompressed_size: usize_to_u64(row_group.raw_bytes, "row group is too large")?,
        compressed_size,
        checksum: crc32::checksum(&block_payload),
        encoded_frame,
    })
}

fn collect_results<W: Write>(
    result_rx: mpsc::Receiver<Result<EncodedBlock>>,
    output: &mut W,
) -> Result<(u64, Vec<BlockMetadata>)> {
    let mut pending = BTreeMap::new();
    let mut next_to_write = 0u64;
    let mut bytes_written = FILE_HEADER_LEN;
    let mut metadata = Vec::new();

    let mut result_count = 0u64;

    while let Ok(encoded) = result_rx.recv() {
        let encoded = encoded?;
        result_count = result_count
            .checked_add(1)
            .ok_or(CompactError::InvalidInput("parallel result count overflow"))?;
        pending.insert(encoded.block_index, encoded);

        while let Some(encoded) = pending.remove(&next_to_write) {
            let block_metadata = BlockMetadata {
                block_index: encoded.block_index,
                encoded_offset: bytes_written,
                row_count: encoded.row_count,
                uncompressed_size: encoded.uncompressed_size,
                compressed_size: encoded.compressed_size,
                checksum: encoded.checksum,
            };

            output.write_all(&encoded.encoded_frame)?;
            bytes_written = bytes_written
                .checked_add(encoded.compressed_size)
                .ok_or(CompactError::InvalidInput("stream size overflow"))?;
            metadata.push(block_metadata);
            next_to_write = next_to_write
                .checked_add(1)
                .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
        }
    }

    if next_to_write != result_count || !pending.is_empty() {
        return Err(CompactError::InvalidInput(
            "parallel block results are incomplete",
        ));
    }

    Ok((result_count, metadata))
}

fn decode_worker_loop(
    schema: Schema,
    job_rx: Arc<Mutex<mpsc::Receiver<DecodeJob>>>,
    result_tx: mpsc::SyncSender<Result<DecodedBlockJob>>,
) {
    loop {
        let job = match job_rx.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => {
                let _ = result_tx.send(Err(CompactError::InvalidInput(
                    "parallel worker pool stopped",
                )));
                break;
            }
        };

        let Ok(job) = job else {
            break;
        };

        if result_tx.send(decode_job(&schema, job)).is_err() {
            break;
        }
    }
}

fn schedule_decode_jobs<R: Read>(
    input: &mut R,
    job_tx: mpsc::SyncSender<DecodeJob>,
) -> Result<u64> {
    let mut next_offset = STREAM_HEADER_LEN as u64;
    let mut expected_block_index = 0u64;
    let mut expected_first_row_index = 0u64;
    let mut metadata = Vec::new();

    loop {
        let frame = match read_next_record_from(input)? {
            StreamRecord::Frame(frame) => frame,
            StreamRecord::Index(index) => {
                if index != metadata {
                    return Err(CompactError::InvalidInput(
                        "footer index does not match scanned blocks",
                    ));
                }
                ensure_reader_eof(input)?;
                break;
            }
            StreamRecord::Eof => {
                return Err(CompactError::InvalidInput(
                    "sealed stream footer is missing",
                ));
            }
        };
        let frame_len = frame.len();
        let decoded = framing::decode_v1(&frame)?;
        if decoded.codec != Codec::ColumnBlock {
            return Err(CompactError::InvalidInput(
                "stream block frame must use column block codec",
            ));
        }

        let parsed = parse_block_payload(&decoded.payload)?;
        if parsed.block_index != expected_block_index {
            return Err(CompactError::InvalidInput(
                "stream block index is not sequential",
            ));
        }

        if parsed.first_row_index != expected_first_row_index {
            return Err(CompactError::InvalidInput(
                "stream block first row index is not sequential",
            ));
        }

        let block_metadata = BlockMetadata {
            block_index: parsed.block_index,
            encoded_offset: next_offset,
            row_count: reader_usize_to_u64(parsed.row_count, "row count is too large")?,
            uncompressed_size: reader_usize_to_u64(parsed.raw_size, "row group is too large")?,
            compressed_size: reader_usize_to_u64(frame_len, "block frame is too large")?,
            checksum: crc32::checksum(&decoded.payload),
        };
        if metadata.len() >= MAX_STREAM_BLOCKS {
            return Err(CompactError::InvalidInput(
                "stream block count exceeds configured limit",
            ));
        }
        let job = DecodeJob {
            block_index: parsed.block_index,
            first_row_index: parsed.first_row_index,
            row_count: parsed.row_count,
            raw_size: parsed.raw_size,
            column_block: parsed.column_block.to_vec(),
        };

        send_decode_job(&job_tx, job)?;
        expected_block_index = expected_block_index
            .checked_add(1)
            .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
        expected_first_row_index = expected_first_row_index
            .checked_add(block_metadata.row_count)
            .ok_or(CompactError::InvalidInput("parallel row count overflow"))?;
        next_offset = next_offset
            .checked_add(block_metadata.compressed_size)
            .ok_or(CompactError::InvalidInput(
                "parallel stream offset overflow",
            ))?;
        metadata.push(block_metadata);
    }

    drop(job_tx);

    Ok(expected_block_index)
}

fn send_decode_job(job_tx: &mpsc::SyncSender<DecodeJob>, job: DecodeJob) -> Result<()> {
    job_tx
        .send(job)
        .map_err(|_| CompactError::InvalidInput("parallel worker pool stopped"))
}

fn decode_job(schema: &Schema, job: DecodeJob) -> Result<DecodedBlockJob> {
    let column_frame = framing::encode_v1(Codec::ColumnBlock, &job.column_block);
    let jsonl = decode_jsonl(&column_frame, schema)?;
    if count_jsonl_rows(&jsonl) != job.row_count {
        return Err(CompactError::InvalidInput(
            "decoded block row count does not match metadata",
        ));
    }

    if jsonl.len() != job.raw_size {
        return Err(CompactError::InvalidInput(
            "decoded block raw size does not match metadata",
        ));
    }

    Ok(DecodedBlockJob {
        block_index: job.block_index,
        first_row_index: job.first_row_index,
        jsonl,
    })
}

fn collect_decode_results<W: Write>(
    result_rx: mpsc::Receiver<Result<DecodedBlockJob>>,
    output: &mut W,
) -> Result<u64> {
    let mut pending = BTreeMap::new();
    let mut next_to_write = 0u64;
    let mut next_row_index = 0u64;

    let mut result_count = 0u64;

    while let Ok(decoded) = result_rx.recv() {
        let decoded = decoded?;
        result_count = result_count
            .checked_add(1)
            .ok_or(CompactError::InvalidInput("parallel result count overflow"))?;
        pending.insert(decoded.block_index, decoded);

        while let Some(decoded) = pending.remove(&next_to_write) {
            if decoded.first_row_index != next_row_index {
                return Err(CompactError::InvalidInput(
                    "decoded block first row index is not sequential",
                ));
            }

            output.write_all(decoded.jsonl.as_bytes())?;
            next_row_index = next_row_index
                .checked_add(count_jsonl_rows(&decoded.jsonl) as u64)
                .ok_or(CompactError::InvalidInput("parallel row count overflow"))?;
            next_to_write = next_to_write
                .checked_add(1)
                .ok_or(CompactError::InvalidInput("parallel block index overflow"))?;
        }
    }

    if next_to_write != result_count || !pending.is_empty() {
        return Err(CompactError::InvalidInput(
            "parallel block results are incomplete",
        ));
    }

    Ok(result_count)
}

fn join_scoped<T>(handle: thread::ScopedJoinHandle<'_, Result<T>>) -> Result<T> {
    handle
        .join()
        .map_err(|_| CompactError::InvalidInput("parallel scheduler panicked"))?
}

fn join_workers(workers: Vec<thread::ScopedJoinHandle<'_, ()>>) -> Result<()> {
    for worker in workers {
        worker
            .join()
            .map_err(|_| CompactError::InvalidInput("parallel worker panicked"))?;
    }
    Ok(())
}

#[derive(Debug)]
struct RowGroupJob {
    block_index: u64,
    first_row_index: u64,
    jsonl: String,
}

#[derive(Debug)]
struct EncodedBlock {
    block_index: u64,
    row_count: u64,
    uncompressed_size: u64,
    compressed_size: u64,
    checksum: u32,
    encoded_frame: Vec<u8>,
}

#[derive(Debug)]
struct DecodeJob {
    block_index: u64,
    first_row_index: u64,
    row_count: usize,
    raw_size: usize,
    column_block: Vec<u8>,
}

#[derive(Debug)]
struct DecodedBlockJob {
    block_index: u64,
    first_row_index: u64,
    jsonl: String,
}

#[derive(Debug, Default)]
struct PendingRowGroup {
    data: String,
    row_count: u64,
    raw_bytes: usize,
}

impl PendingRowGroup {
    fn push_line(&mut self, line: &str) {
        self.data.push_str(line);
        self.row_count += 1;
        self.raw_bytes += line.len();
    }

    fn would_exceed(&self, line: &str, options: BlockOptions) -> bool {
        if self.is_empty() {
            return false;
        }

        self.row_count + 1 > options.max_rows_per_block as u64
            || self.raw_bytes + line.len() > options.max_uncompressed_bytes_per_block
    }

    fn reached_limit(&self, options: BlockOptions) -> bool {
        self.row_count >= options.max_rows_per_block as u64
            || self.raw_bytes >= options.max_uncompressed_bytes_per_block
    }

    fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    fn take_job(&mut self, block_index: u64, first_row_index: u64) -> (RowGroupJob, u64) {
        let row_count = self.row_count;
        let jsonl = std::mem::take(&mut self.data);
        self.row_count = 0;
        self.raw_bytes = 0;

        (
            RowGroupJob {
                block_index,
                first_row_index,
                jsonl,
            },
            row_count,
        )
    }
}

fn usize_to_u64(value: usize, err: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CompactError::InvalidInput(err))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io::Cursor;
    use std::task::{Context, Poll, Waker};

    use super::{
        ParallelDecodeOptions, ParallelOptions, decode_jsonl_stream_parallel,
        decode_jsonl_stream_parallel_async_writer, encode_jsonl_stream_parallel,
    };
    use crate::limits::MAX_PARALLEL_WORKERS;
    use crate::streaming::{BlockOptions, decode_jsonl_stream, inspect_stream};
    use crate::{CompactError, schema::Schema};

    fn schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
  - name: level
    type: string
    codec: rle
"#,
        )
        .unwrap()
    }

    fn input(rows: usize) -> String {
        let mut jsonl = String::new();
        for row in 0..rows {
            let level = if row % 2 == 0 { "INFO" } else { "WARN" };
            jsonl.push_str(&format!(
                "{{\"ts\":{},\"level\":\"{}\"}}\n",
                1_000 + row,
                level
            ));
        }
        jsonl
    }

    fn run_ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    #[test]
    fn parallel_encode_roundtrips_as_cmp2_stream() {
        let input = input(25);
        let options = ParallelOptions {
            worker_count: 4,
            block_options: BlockOptions {
                max_rows_per_block: 3,
                max_uncompressed_bytes_per_block: 1024,
            },
        };

        let encoded =
            encode_jsonl_stream_parallel(Cursor::new(&input), Vec::new(), schema(), options)
                .unwrap();
        let decoded = decode_jsonl_stream(Cursor::new(&encoded), Vec::new(), schema()).unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), input);
    }

    #[test]
    fn parallel_encode_preserves_block_order_and_footer_index() {
        let input = input(17);
        let options = ParallelOptions {
            worker_count: 8,
            block_options: BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        };

        let encoded =
            encode_jsonl_stream_parallel(Cursor::new(input), Vec::new(), schema(), options)
                .unwrap();
        let inspect = inspect_stream(Cursor::new(encoded)).unwrap();

        assert_eq!(inspect.blocks.len(), 9);
        assert_eq!(
            inspect
                .blocks
                .iter()
                .map(|block| block.block_index)
                .collect::<Vec<_>>(),
            (0..9).collect::<Vec<_>>()
        );
        assert_eq!(inspect.footer_index.as_ref(), Some(&inspect.blocks));
    }

    #[test]
    fn invalid_worker_count_is_rejected_before_output() {
        let options = ParallelOptions {
            worker_count: 0,
            block_options: BlockOptions::default(),
        };
        let err =
            encode_jsonl_stream_parallel(Cursor::new(input(1)), Vec::new(), schema(), options)
                .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("parallel worker count must be greater than zero")
        ));
    }

    #[test]
    fn excessive_worker_count_is_rejected_before_output() {
        let error = encode_jsonl_stream_parallel(
            Cursor::new(input(1)),
            Vec::new(),
            schema(),
            ParallelOptions {
                worker_count: MAX_PARALLEL_WORKERS + 1,
                block_options: BlockOptions::default(),
            },
        )
        .unwrap_err();

        assert!(matches!(error, CompactError::InvalidInput(_)));
    }

    #[test]
    fn worker_encode_error_is_returned() {
        let options = ParallelOptions {
            worker_count: 2,
            block_options: BlockOptions {
                max_rows_per_block: 1,
                max_uncompressed_bytes_per_block: 1024,
            },
        };
        let err = encode_jsonl_stream_parallel(
            Cursor::new("{\"missing_ts\":1,\"level\":\"INFO\"}\n"),
            Vec::new(),
            schema(),
            options,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl row missing schema column")
        ));
    }

    #[test]
    fn parallel_decode_roundtrips_cmp2_stream() {
        let input = input(33);
        let encoded = encode_jsonl_stream_parallel(
            Cursor::new(&input),
            Vec::new(),
            schema(),
            ParallelOptions {
                worker_count: 4,
                block_options: BlockOptions {
                    max_rows_per_block: 3,
                    max_uncompressed_bytes_per_block: 1024,
                },
            },
        )
        .unwrap();

        let decoded = decode_jsonl_stream_parallel(
            Cursor::new(encoded),
            Vec::new(),
            schema(),
            ParallelDecodeOptions { worker_count: 4 },
        )
        .unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), input);
    }

    #[test]
    fn async_parallel_decode_writes_ordered_bounded_blocks() {
        let input = input(12);
        let encoded = encode_jsonl_stream_parallel(
            Cursor::new(&input),
            Vec::new(),
            schema(),
            ParallelOptions {
                worker_count: 3,
                block_options: BlockOptions {
                    max_rows_per_block: 2,
                    max_uncompressed_bytes_per_block: 1024,
                },
            },
        )
        .unwrap();

        let decoded = run_ready(decode_jsonl_stream_parallel_async_writer(
            Cursor::new(encoded),
            Vec::new(),
            schema(),
            ParallelDecodeOptions { worker_count: 3 },
        ))
        .unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), input);
    }

    #[test]
    fn parallel_decode_rejects_invalid_worker_count() {
        let err = decode_jsonl_stream_parallel(
            Cursor::new(Vec::<u8>::new()),
            Vec::new(),
            schema(),
            ParallelDecodeOptions { worker_count: 0 },
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("parallel worker count must be greater than zero")
        ));
    }

    #[test]
    fn parallel_decode_rejects_corrupted_later_block() {
        let input = input(8);
        let mut encoded = encode_jsonl_stream_parallel(
            Cursor::new(&input),
            Vec::new(),
            schema(),
            ParallelOptions {
                worker_count: 2,
                block_options: BlockOptions {
                    max_rows_per_block: 2,
                    max_uncompressed_bytes_per_block: 1024,
                },
            },
        )
        .unwrap();
        let inspect = inspect_stream(Cursor::new(&encoded)).unwrap();
        let second = &inspect.blocks[1];
        let corrupt_at = second.encoded_offset as usize + second.compressed_size as usize - 1;
        encoded[corrupt_at] ^= 0xff;

        let err = decode_jsonl_stream_parallel(
            Cursor::new(encoded),
            Vec::new(),
            schema(),
            ParallelDecodeOptions { worker_count: 2 },
        )
        .unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput(_)));
    }

    #[test]
    fn parallel_decode_rejects_trailing_bytes_after_footer() {
        let mut encoded = encode_jsonl_stream_parallel(
            Cursor::new(input(2)),
            Vec::new(),
            schema(),
            ParallelOptions::default(),
        )
        .unwrap();
        encoded.push(0xff);

        let error = decode_jsonl_stream_parallel(
            Cursor::new(encoded),
            Vec::new(),
            schema(),
            ParallelDecodeOptions::default(),
        )
        .unwrap_err();

        assert!(matches!(error, CompactError::InvalidInput(_)));
    }

    #[test]
    fn repeated_parallel_roundtrips_preserve_order_under_stress() {
        let input = input(128);
        for worker_count in [1, 2, 4, 8] {
            for _ in 0..8 {
                let encoded = encode_jsonl_stream_parallel(
                    Cursor::new(&input),
                    Vec::new(),
                    schema(),
                    ParallelOptions {
                        worker_count,
                        block_options: BlockOptions {
                            max_rows_per_block: 4,
                            max_uncompressed_bytes_per_block: 1024,
                        },
                    },
                )
                .unwrap();
                let decoded = decode_jsonl_stream_parallel(
                    Cursor::new(encoded),
                    Vec::new(),
                    schema(),
                    ParallelDecodeOptions { worker_count },
                )
                .unwrap();

                assert_eq!(String::from_utf8(decoded).unwrap(), input);
            }
        }
    }
}
