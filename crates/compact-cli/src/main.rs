use std::fs::{self, OpenOptions};
use std::io::{BufReader, Cursor, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use compact_core::{Codec, EncodeConfig, Transform, ValueType};

#[derive(Debug, Parser)]
#[command(name = "compact", version, about = "Portable compression CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Encode {
        input: String,
        output: String,
        #[arg(long, value_enum, default_value_t = CliCodec::Rle)]
        codec: CliCodec,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
        #[arg(long, value_enum, default_value_t = CliFormat::V2)]
        format: CliFormat,
    },
    Decode {
        input: String,
        output: String,
        #[arg(long, value_enum, default_value_t = CliCodec::Rle)]
        codec: CliCodec,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        filter_column: Option<String>,
        #[arg(long, value_enum)]
        filter_op: Option<CliFilterOp>,
        #[arg(long)]
        filter_value: Option<u64>,
        #[arg(long, value_enum, default_value_t = CliFormat::V2)]
        format: CliFormat,
    },
    Inspect {
        input: String,
    },
    /// Validate storage structure and checksums without decoding user data.
    Validate {
        input: String,
    },
    /// Check whether files written with one schema revision are readable by another.
    SchemaCheck {
        writer_schema: String,
        reader_schema: String,
    },
    /// Decode a columnar file and apply a checked schema evolution plan.
    EvolveDecode {
        input: String,
        output: String,
        #[arg(long)]
        writer_schema: String,
        #[arg(long)]
        reader_schema: String,
    },
    /// Plan or execute copy-on-write repair for a recoverable storage file.
    Repair {
        input: String,
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Migrate external schema metadata to the stable-ID contract.
    MetadataMigrate {
        input: String,
        #[arg(long)]
        output: Option<String>,
        #[arg(long = "column-id")]
        column_ids: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Benchmark deterministic repair planning and execution.
    RepairBench {
        input: String,
        #[arg(long, default_value_t = 10)]
        iterations: usize,
    },
    Bench {
        input: String,
        #[arg(long)]
        schema: String,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
        #[arg(long, value_enum, default_value_t = CliFormat::V2)]
        format: CliFormat,
    },
    ParallelBench {
        input: String,
        #[arg(long)]
        schema: String,
        #[arg(long)]
        workers: Option<usize>,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
    },
    SearchEncode {
        input: String,
        output: String,
        #[arg(long, default_value_t = 16)]
        skip_step: usize,
    },
    SearchInspect {
        input: String,
    },
    SearchLookup {
        input: String,
        #[arg(long)]
        term: String,
    },
    SearchSeek {
        input: String,
        #[arg(long)]
        term: String,
        #[arg(long)]
        doc_id: u64,
    },
    SearchBench {
        input: String,
        #[arg(long, default_value_t = 16)]
        skip_step: usize,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
    },
    StreamAppend {
        input: String,
        output: String,
        #[arg(long)]
        schema: String,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
    },
    StreamRecover {
        input: String,
    },
    StreamReplay {
        input: String,
        output: String,
        #[arg(long)]
        schema: String,
    },
    StreamRoll {
        input: String,
        output_dir: String,
        #[arg(long)]
        schema: String,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
        #[arg(long, default_value_t = 64 * 1024 * 1024)]
        max_segment_bytes: usize,
        #[arg(long, default_value_t = 1024)]
        max_blocks: usize,
    },
    StreamBench {
        input: String,
        #[arg(long)]
        schema: String,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
    },
    SnapshotEncode {
        input: String,
        output: String,
        #[arg(long)]
        checkpoint_id: u64,
    },
    SnapshotDecode {
        input: String,
        output: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliCodec {
    Rle,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliFormat {
    V2,
    V3,
    V4,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliFilterOp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
    IsNull,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Encode {
            input,
            output,
            codec,
            schema,
            block_rows,
            block_bytes,
            format,
        } => {
            if let Some(schema_path) = schema {
                let schema = load_schema(&schema_path)?;
                match format {
                    CliFormat::V2 => {
                        let options = block_options(block_rows, block_bytes)
                            .context("invalid streaming block options")?;
                        let input_file = fs::File::open(&input)
                            .with_context(|| format!("failed to open {input}"))?;
                        atomic_write_with(&output, |output_file| {
                            compact_core::streaming::encode_jsonl_stream(
                                BufReader::new(input_file),
                                output_file,
                                schema,
                                options,
                            )
                            .context("failed to stream encode JSONL input")?;
                            Ok(())
                        })?;
                    }
                    CliFormat::V3 => {
                        let input_text = fs::read_to_string(&input)
                            .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
                        let encoded = compact_core::io::v3::encode_jsonl(&input_text, &schema)
                            .context("failed to encode CMP3 JSONL input")?;
                        fs::write(&output, encoded)
                            .with_context(|| format!("failed to write {output}"))?;
                    }
                    CliFormat::V4 => {
                        let options = cmp4_options(block_rows, block_bytes)
                            .context("invalid CMP4 options")?;
                        let input_text = fs::read_to_string(&input)
                            .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
                        let encoded =
                            compact_core::io::v4::encode_jsonl(&input_text, &schema, options)
                                .context("failed to encode CMP4 JSONL input")?;
                        fs::write(&output, encoded)
                            .with_context(|| format!("failed to write {output}"))?;
                    }
                }
            } else {
                let config = codec.byte_config();
                let input_bytes =
                    fs::read(&input).with_context(|| format!("failed to read {input}"))?;
                let encoded = compact_core::encode_bytes_frame(&config, &input_bytes)
                    .context("failed to encode input")?;

                fs::write(&output, encoded).with_context(|| format!("failed to write {output}"))?;
            }

            let input_len = fs::metadata(&input)
                .with_context(|| format!("failed to stat {input}"))?
                .len();
            let output_len = fs::metadata(&output)
                .with_context(|| format!("failed to stat {output}"))?
                .len();
            print_ratio(input_len, output_len);
        }
        Commands::Decode {
            input,
            output,
            codec,
            schema,
            project,
            filter_column,
            filter_op,
            filter_value,
            format,
        } => {
            if let Some(schema_path) = schema {
                let schema = load_schema(&schema_path)?;
                match format {
                    CliFormat::V2 => {
                        let input_file = fs::File::open(&input)
                            .with_context(|| format!("failed to open {input}"))?;
                        atomic_write_with(&output, |output_file| {
                            compact_core::streaming::decode_jsonl_stream(
                                input_file,
                                output_file,
                                schema,
                            )
                            .context("failed to stream decode JSONL input")?;
                            Ok(())
                        })?;
                    }
                    CliFormat::V3 => {
                        let encoded =
                            fs::read(&input).with_context(|| format!("failed to read {input}"))?;
                        let decoded = compact_core::io::v3::decode_jsonl(&encoded, &schema)
                            .context("failed to decode CMP3 JSONL input")?;
                        fs::write(&output, decoded)
                            .with_context(|| format!("failed to write {output}"))?;
                    }
                    CliFormat::V4 => {
                        let encoded =
                            fs::read(&input).with_context(|| format!("failed to read {input}"))?;
                        let projection = parse_projection(project.as_deref());
                        let predicate = parse_predicate(filter_column, filter_op, filter_value)
                            .context("invalid CMP4 filter")?;
                        let projection_refs =
                            projection.iter().map(String::as_str).collect::<Vec<_>>();
                        let decoded = compact_core::io::v4::scan_jsonl(
                            &encoded,
                            &schema,
                            &projection_refs,
                            predicate.as_ref(),
                        )
                        .context("failed to scan CMP4 JSONL input")?
                        .jsonl;
                        fs::write(&output, decoded)
                            .with_context(|| format!("failed to write {output}"))?;
                    }
                }
            } else {
                let frame = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
                let config = codec.byte_config();
                let decoded = compact_core::decode_bytes_frame(&config, &frame)
                    .context("failed to decode input")?;

                fs::write(&output, decoded).with_context(|| format!("failed to write {output}"))?;
            }
        }
        Commands::Inspect { input } => {
            let frame = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            inspect_file(&frame)?;
        }
        Commands::Validate { input } => {
            let file = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let report =
                compact_core::storage::validate(&file).context("storage validation failed")?;

            println!("valid: true");
            println!("format: {}", report.format);
            println!("version: {}", report.format.version());
            println!("file_bytes: {}", report.file_size);
            println!("storage_units: {}", report.storage_units);
            if let Some(total_rows) = report.total_rows {
                println!("rows: {total_rows}");
            }
            if let Some(has_footer_index) = report.has_footer_index {
                println!("footer_index: {has_footer_index}");
            }
        }
        Commands::SchemaCheck {
            writer_schema,
            reader_schema,
        } => {
            let writer = load_schema_revision(&writer_schema)?;
            let reader = load_schema_revision(&reader_schema)?;
            let assessment = compact_core::schema::evolution::assess(&writer, &reader)
                .context("failed to assess schema evolution")?;

            println!("writer_revision: {}", assessment.writer_revision);
            println!("reader_revision: {}", assessment.reader_revision);
            println!("compatible: {}", assessment.is_compatible());
            println!("actions: {}", assessment.actions.len());
            for action in &assessment.actions {
                println!("action: {action:?}");
            }
            for issue in &assessment.issues {
                println!("issue: {issue:?}");
            }
            if !assessment.is_compatible() {
                anyhow::bail!("schema revisions are incompatible");
            }
        }
        Commands::EvolveDecode {
            input,
            output,
            writer_schema,
            reader_schema,
        } => {
            let file = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let writer = load_schema_revision(&writer_schema)?;
            let reader = load_schema_revision(&reader_schema)?;
            let decoded = compact_core::schema::evolution::decode_jsonl(&file, &writer, &reader)
                .context("failed to decode with schema evolution")?;

            fs::write(&output, decoded).with_context(|| format!("failed to write {output}"))?;
        }
        Commands::Repair {
            input,
            output,
            dry_run,
        } => {
            let source = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let plan =
                compact_core::storage::repair::plan(&source).context("file is not repairable")?;

            println!("format: {}", plan.format);
            println!("action: {:?}", plan.action);
            println!("source_bytes: {}", plan.source_len);
            println!("recoverable_bytes: {}", plan.recoverable_len);
            println!("discarded_bytes: {}", plan.discarded_bytes);
            println!("recovered_units: {}", plan.recovered_units);
            println!("recovered_rows: {}", plan.recovered_rows);

            if !dry_run {
                let output =
                    output.context("--output is required unless --dry-run is specified")?;
                let repaired = compact_core::storage::repair::execute(&source, &plan)
                    .context("failed to execute repair plan")?;
                write_new_file(&output, &repaired)
                    .with_context(|| format!("failed to create repaired file {output}"))?;
            }
        }
        Commands::MetadataMigrate {
            input,
            output,
            column_ids,
            dry_run,
        } => {
            let source = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let assignments = parse_migration_assignments(&column_ids)?;
            let plan = compact_core::storage::migration::plan(&source, &assignments)
                .context("metadata is not migratable")?;

            println!("source_version: {}", plan.source_version);
            println!("target_version: {}", plan.target_version);
            println!("action: {:?}", plan.action);
            println!("columns: {}", plan.column_count);

            if !dry_run {
                let output =
                    output.context("--output is required unless --dry-run is specified")?;
                let migrated = compact_core::storage::migration::execute(&source, &plan)
                    .context("failed to execute metadata migration")?;
                write_new_file(&output, &migrated)
                    .with_context(|| format!("failed to create migrated metadata {output}"))?;
            }
        }
        Commands::RepairBench { input, iterations } => {
            if iterations == 0 {
                anyhow::bail!("repair benchmark iterations must be positive");
            }
            let source = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let plan_start = Instant::now();
            let plan = compact_core::storage::repair::plan(&source)
                .context("benchmark input is not repairable")?;
            let plan_elapsed = plan_start.elapsed();
            let execute_start = Instant::now();
            let mut output_bytes = 0usize;

            for _ in 0..iterations {
                let repaired = compact_core::storage::repair::execute(&source, &plan)
                    .context("repair benchmark execution failed")?;
                output_bytes = repaired.len();
            }
            let execute_elapsed = execute_start.elapsed();
            let processed_bytes = (source.len() as u64)
                .checked_mul(iterations as u64)
                .context("repair benchmark byte count overflow")?;

            println!("mode: repair");
            println!("format: {}", plan.format);
            println!("action: {:?}", plan.action);
            println!("iterations: {iterations}");
            println!("input_bytes: {}", source.len());
            println!("output_bytes: {output_bytes}");
            println!("recovered_units: {}", plan.recovered_units);
            println!("recovered_rows: {}", plan.recovered_rows);
            println!("plan_ms: {:.3}", plan_elapsed.as_secs_f64() * 1000.0);
            println!("execute_ms: {:.3}", execute_elapsed.as_secs_f64() * 1000.0);
            println!(
                "execute_mib_s: {:.3}",
                mib_per_second(processed_bytes, execute_elapsed.as_secs_f64())
            );
        }
        Commands::Bench {
            input,
            schema,
            block_rows,
            block_bytes,
            format,
        } => {
            let schema = load_schema(&schema)?;
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
            let input_bytes = input_text.len() as u64;
            let encode_start = Instant::now();
            let encoded = match format {
                CliFormat::V2 => {
                    let options = block_options(block_rows, block_bytes)
                        .context("invalid streaming block options")?;
                    compact_core::streaming::encode_jsonl_stream(
                        BufReader::new(Cursor::new(input_text.as_bytes())),
                        Vec::new(),
                        schema.clone(),
                        options,
                    )
                    .context("failed to stream encode JSONL benchmark input")?
                }
                CliFormat::V3 => compact_core::io::v3::encode_jsonl(&input_text, &schema)
                    .context("failed to encode CMP3 benchmark input")?,
                CliFormat::V4 => {
                    let options =
                        cmp4_options(block_rows, block_bytes).context("invalid CMP4 options")?;
                    compact_core::io::v4::encode_jsonl(&input_text, &schema, options)
                        .context("failed to encode CMP4 benchmark input")?
                }
            };
            let encode_elapsed = encode_start.elapsed();
            let decode_start = Instant::now();
            let decoded = match format {
                CliFormat::V2 => compact_core::streaming::decode_jsonl_stream(
                    Cursor::new(&encoded),
                    Vec::new(),
                    schema.clone(),
                )
                .context("failed to stream decode JSONL benchmark input")?,
                CliFormat::V3 => compact_core::io::v3::decode_jsonl(&encoded, &schema)
                    .context("failed to decode CMP3 benchmark input")?
                    .into_bytes(),
                CliFormat::V4 => compact_core::io::v4::decode_jsonl(&encoded, &schema)
                    .context("failed to decode CMP4 benchmark input")?
                    .into_bytes(),
            };
            let decode_elapsed = decode_start.elapsed();

            if decoded != input_text.as_bytes() {
                anyhow::bail!("benchmark roundtrip mismatch");
            }

            match format {
                CliFormat::V2 => {
                    let options = block_options(block_rows, block_bytes)?;
                    let inspect = compact_core::streaming::inspect_stream(Cursor::new(&encoded))
                        .context("failed to inspect benchmark stream")?;
                    println!("mode: stream");
                    println!("block_rows: {}", options.max_rows_per_block);
                    println!("block_bytes: {}", options.max_uncompressed_bytes_per_block);
                    println!("blocks: {}", inspect.blocks.len());
                    println!("rows: {}", inspect.total_rows);
                }
                CliFormat::V3 => {
                    let inspect = compact_core::io::v3::inspect_jsonl(&encoded)
                        .context("failed to inspect CMP3 benchmark")?;
                    println!("mode: v3");
                    println!("blocks: 1");
                    println!("rows: {}", inspect.row_count);
                }
                CliFormat::V4 => {
                    let footer = compact_core::io::v4::inspect_footer(&encoded)
                        .context("failed to inspect CMP4 benchmark")?;
                    let projection_start = Instant::now();
                    let projection = compact_core::io::v4::decode_jsonl_projected(
                        &encoded,
                        &schema,
                        &schema.columns[0..1]
                            .iter()
                            .map(|column| column.name.as_str())
                            .collect::<Vec<_>>(),
                    )
                    .context("failed to run CMP4 projection benchmark")?;
                    let projection_elapsed = projection_start.elapsed();

                    println!("mode: v4");
                    println!("row_groups: {}", footer.row_groups.len());
                    println!("rows: {}", footer.total_row_count);
                    println!("projected_columns: 1");
                    println!("projected_bytes: {}", projection.len());
                    println!(
                        "projected_decode_ms: {:.3}",
                        projection_elapsed.as_secs_f64() * 1000.0
                    );
                }
            }
            println!("input_bytes: {}", input_bytes);
            println!("encoded_bytes: {}", encoded.len());
            println!(
                "compression_ratio: {:.4}",
                compression_ratio(input_bytes, encoded.len() as u64)
            );
            println!("encode_ms: {:.3}", encode_elapsed.as_secs_f64() * 1000.0);
            println!("decode_ms: {:.3}", decode_elapsed.as_secs_f64() * 1000.0);
            println!(
                "encode_mib_s: {:.3}",
                mib_per_second(input_bytes, encode_elapsed.as_secs_f64())
            );
            println!(
                "decode_mib_s: {:.3}",
                mib_per_second(input_bytes, decode_elapsed.as_secs_f64())
            );
        }
        Commands::ParallelBench {
            input,
            schema,
            workers,
            block_rows,
            block_bytes,
        } => {
            let schema = load_schema(&schema)?;
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
            let input_bytes = input_text.len() as u64;
            let block_options =
                block_options(block_rows, block_bytes).context("invalid parallel block options")?;
            let worker_count = workers.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1)
            });

            let sequential_start = Instant::now();
            let sequential = compact_core::streaming::encode_jsonl_stream(
                BufReader::new(Cursor::new(input_text.as_bytes())),
                Vec::new(),
                schema.clone(),
                block_options,
            )
            .context("failed to run sequential CMP2 benchmark")?;
            let sequential_elapsed = sequential_start.elapsed();

            let parallel_start = Instant::now();
            let parallel = compact_core::parallel::encode_jsonl_stream_parallel(
                BufReader::new(Cursor::new(input_text.as_bytes())),
                Vec::new(),
                schema.clone(),
                compact_core::parallel::ParallelOptions {
                    worker_count,
                    block_options,
                },
            )
            .context("failed to run parallel CMP2 benchmark")?;
            let parallel_elapsed = parallel_start.elapsed();

            let sequential_decode_start = Instant::now();
            let sequential_decoded = compact_core::streaming::decode_jsonl_stream(
                Cursor::new(&parallel),
                Vec::new(),
                schema.clone(),
            )
            .context("failed to sequentially decode parallel benchmark output")?;
            let sequential_decode_elapsed = sequential_decode_start.elapsed();

            let parallel_decode_start = Instant::now();
            let parallel_decoded = compact_core::parallel::decode_jsonl_stream_parallel(
                Cursor::new(&parallel),
                Vec::new(),
                schema,
                compact_core::parallel::ParallelDecodeOptions { worker_count },
            )
            .context("failed to parallel decode benchmark output")?;
            let parallel_decode_elapsed = parallel_decode_start.elapsed();

            if sequential_decoded != input_text.as_bytes()
                || parallel_decoded != input_text.as_bytes()
            {
                anyhow::bail!("parallel benchmark roundtrip mismatch");
            }

            let inspect = compact_core::streaming::inspect_stream(Cursor::new(&parallel))
                .context("failed to inspect parallel benchmark output")?;
            println!("mode: parallel");
            println!("workers: {}", worker_count);
            println!("block_rows: {}", block_options.max_rows_per_block);
            println!(
                "block_bytes: {}",
                block_options.max_uncompressed_bytes_per_block
            );
            println!("blocks: {}", inspect.blocks.len());
            println!("rows: {}", inspect.total_rows);
            println!("input_bytes: {}", input_bytes);
            println!("sequential_encoded_bytes: {}", sequential.len());
            println!("parallel_encoded_bytes: {}", parallel.len());
            println!(
                "compression_ratio: {:.4}",
                compression_ratio(input_bytes, parallel.len() as u64)
            );
            println!(
                "sequential_encode_ms: {:.3}",
                sequential_elapsed.as_secs_f64() * 1000.0
            );
            println!(
                "parallel_encode_ms: {:.3}",
                parallel_elapsed.as_secs_f64() * 1000.0
            );
            println!(
                "sequential_encode_mib_s: {:.3}",
                mib_per_second(input_bytes, sequential_elapsed.as_secs_f64())
            );
            println!(
                "parallel_encode_mib_s: {:.3}",
                mib_per_second(input_bytes, parallel_elapsed.as_secs_f64())
            );
            println!(
                "sequential_decode_ms: {:.3}",
                sequential_decode_elapsed.as_secs_f64() * 1000.0
            );
            println!(
                "parallel_decode_ms: {:.3}",
                parallel_decode_elapsed.as_secs_f64() * 1000.0
            );
            println!(
                "sequential_decode_mib_s: {:.3}",
                mib_per_second(input_bytes, sequential_decode_elapsed.as_secs_f64())
            );
            println!(
                "parallel_decode_mib_s: {:.3}",
                mib_per_second(input_bytes, parallel_decode_elapsed.as_secs_f64())
            );
            println!(
                "encode_speedup: {:.3}",
                sequential_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
            );
            println!(
                "decode_speedup: {:.3}",
                sequential_decode_elapsed.as_secs_f64() / parallel_decode_elapsed.as_secs_f64()
            );
        }
        Commands::SearchEncode {
            input,
            output,
            skip_step,
        } => {
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 search postings"))?;
            let entries = parse_search_postings(&input_text)?;
            let encoded = compact_core::search::dictionary::encode_dictionary(&entries, skip_step)
                .context("failed to encode search dictionary")?;
            fs::write(&output, encoded).with_context(|| format!("failed to write {output}"))?;

            let input_len = fs::metadata(&input)
                .with_context(|| format!("failed to stat {input}"))?
                .len();
            let output_len = fs::metadata(&output)
                .with_context(|| format!("failed to stat {output}"))?
                .len();
            print_ratio(input_len, output_len);
        }
        Commands::SearchInspect { input } => {
            let encoded = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            inspect_search_dictionary(&encoded)?;
        }
        Commands::SearchLookup { input, term } => {
            let encoded = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let postings = compact_core::search::dictionary::lookup_term(&encoded, &term)
                .context("failed to lookup search term")?
                .unwrap_or_default();
            println!("term: {term}");
            println!("documents: {}", postings.len());
            for posting in postings {
                println!(
                    "doc id={} freq={} positions={}",
                    posting.doc_id,
                    posting.positions.len(),
                    join_u64s(&posting.positions)
                );
            }
        }
        Commands::SearchSeek {
            input,
            term,
            doc_id,
        } => {
            let encoded = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            match compact_core::search::dictionary::seek_term_doc(&encoded, &term, doc_id)
                .context("failed to seek search term/docID")?
            {
                Some(posting) => {
                    println!("found: true");
                    println!("term: {term}");
                    println!("doc_id: {}", posting.doc_id);
                    println!("frequency: {}", posting.positions.len());
                    println!("positions: {}", join_u64s(&posting.positions));
                }
                None => println!("found: false"),
            }
        }
        Commands::SearchBench {
            input,
            skip_step,
            top_k,
        } => {
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 search postings"))?;
            let input_bytes = input_text.len() as u64;
            let entries = parse_search_postings(&input_text)?;

            let encode_start = Instant::now();
            let encoded = compact_core::search::dictionary::encode_dictionary(&entries, skip_step)
                .context("failed to encode search benchmark dictionary")?;
            let encode_elapsed = encode_start.elapsed();

            let inspect_start = Instant::now();
            let index = compact_core::search::dictionary::inspect_dictionary(&encoded)
                .context("failed to inspect search benchmark dictionary")?;
            let inspect_elapsed = inspect_start.elapsed();

            let terms = index
                .entries
                .iter()
                .map(|entry| entry.term.as_str())
                .collect::<Vec<_>>();
            let top_k_start = Instant::now();
            let top_hits =
                compact_core::search::query::top_k_by_term_frequency(&encoded, &terms, top_k)
                    .context("failed to run search top-k benchmark")?;
            let top_k_elapsed = top_k_start.elapsed();

            println!("mode: search");
            println!("terms: {}", index.term_count);
            println!("postings_bytes: {}", index.postings_bytes);
            println!("input_bytes: {}", input_bytes);
            println!("encoded_bytes: {}", encoded.len());
            println!(
                "compression_ratio: {:.4}",
                compression_ratio(input_bytes, encoded.len() as u64)
            );
            println!("encode_ms: {:.3}", encode_elapsed.as_secs_f64() * 1000.0);
            println!("inspect_ms: {:.3}", inspect_elapsed.as_secs_f64() * 1000.0);
            println!("top_k: {}", top_hits.len());
            println!("top_k_ms: {:.3}", top_k_elapsed.as_secs_f64() * 1000.0);
        }
        Commands::StreamAppend {
            input,
            output,
            schema,
            block_rows,
            block_bytes,
        } => {
            let schema = load_schema(&schema)?;
            let options = block_options(block_rows, block_bytes)
                .context("invalid append stream block options")?;
            let existing = match fs::read(&output) {
                Ok(existing) => existing,
                Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to read existing append stream {output}")
                    });
                }
            };
            let input_file =
                fs::File::open(&input).with_context(|| format!("failed to open {input}"))?;
            let encoded = compact_core::streaming::append_jsonl_stream(
                &existing,
                BufReader::new(input_file),
                schema,
                options,
            )
            .context("failed to append JSONL stream")?;
            atomic_write_bytes(&output, &encoded)?;
        }
        Commands::StreamRecover { input } => {
            let data = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            inspect_append_recovery(&data)?;
        }
        Commands::StreamReplay {
            input,
            output,
            schema,
        } => {
            let schema = load_schema(&schema)?;
            let data = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let decoded =
                compact_core::streaming::replay_jsonl_append_stream(&data, Vec::new(), schema)
                    .context("failed to replay append stream")?;
            fs::write(&output, decoded).with_context(|| format!("failed to write {output}"))?;
        }
        Commands::StreamRoll {
            input,
            output_dir,
            schema,
            block_rows,
            block_bytes,
            max_segment_bytes,
            max_blocks,
        } => {
            let schema = load_schema(&schema)?;
            let options = block_options(block_rows, block_bytes)
                .context("invalid rolling stream block options")?;
            let rolling = compact_core::streaming::RollingOptions {
                max_segment_bytes,
                max_blocks_per_segment: max_blocks,
            }
            .validate()
            .context("invalid rolling options")?;
            let input_file =
                fs::File::open(&input).with_context(|| format!("failed to open {input}"))?;
            let segments = compact_core::streaming::roll_jsonl_append_segments(
                BufReader::new(input_file),
                schema,
                options,
                rolling,
            )
            .context("failed to roll append stream segments")?;

            fs::create_dir_all(&output_dir)
                .with_context(|| format!("failed to create {output_dir}"))?;
            for (index, segment) in segments.iter().enumerate() {
                let path = format!("{output_dir}/segment-{index:05}.cmp");
                fs::write(&path, segment).with_context(|| format!("failed to write {path}"))?;
            }
            println!("segments: {}", segments.len());
        }
        Commands::StreamBench {
            input,
            schema,
            block_rows,
            block_bytes,
        } => {
            let schema = load_schema(&schema)?;
            let options = block_options(block_rows, block_bytes)
                .context("invalid append benchmark block options")?;
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
            let input_bytes = input_text.len() as u64;

            let append_start = Instant::now();
            let encoded = compact_core::streaming::append_jsonl_stream(
                &[],
                Cursor::new(input_text.as_bytes()),
                schema.clone(),
                options,
            )
            .context("failed to append benchmark JSONL")?;
            let append_elapsed = append_start.elapsed();

            let recovery_start = Instant::now();
            let recovery = compact_core::streaming::recover_append_stream(&encoded)
                .context("failed to recover benchmark append stream")?;
            let recovery_elapsed = recovery_start.elapsed();

            let replay_start = Instant::now();
            let decoded =
                compact_core::streaming::replay_jsonl_append_stream(&encoded, Vec::new(), schema)
                    .context("failed to replay benchmark append stream")?;
            let replay_elapsed = replay_start.elapsed();

            if decoded != input_text.as_bytes() {
                anyhow::bail!("append benchmark roundtrip mismatch");
            }

            println!("mode: append-stream");
            println!("blocks: {}", recovery.blocks.len());
            println!("rows: {}", recovery.total_rows);
            println!("input_bytes: {}", input_bytes);
            println!("encoded_bytes: {}", encoded.len());
            println!(
                "compression_ratio: {:.4}",
                compression_ratio(input_bytes, encoded.len() as u64)
            );
            println!("append_ms: {:.3}", append_elapsed.as_secs_f64() * 1000.0);
            println!(
                "recovery_ms: {:.3}",
                recovery_elapsed.as_secs_f64() * 1000.0
            );
            println!("replay_ms: {:.3}", replay_elapsed.as_secs_f64() * 1000.0);
            println!(
                "append_mib_s: {:.3}",
                mib_per_second(input_bytes, append_elapsed.as_secs_f64())
            );
            println!(
                "replay_mib_s: {:.3}",
                mib_per_second(input_bytes, replay_elapsed.as_secs_f64())
            );
        }
        Commands::SnapshotEncode {
            input,
            output,
            checkpoint_id,
        } => {
            let state = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let encoded = compact_core::streaming::encode_snapshot(checkpoint_id, &state)
                .context("failed to encode snapshot")?;
            fs::write(&output, encoded).with_context(|| format!("failed to write {output}"))?;
            print_ratio(state.len() as u64, fs::metadata(&output)?.len());
        }
        Commands::SnapshotDecode { input, output } => {
            let encoded = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let snapshot = compact_core::streaming::decode_snapshot(&encoded)
                .context("failed to decode snapshot")?;
            fs::write(&output, snapshot.state)
                .with_context(|| format!("failed to write {output}"))?;
            println!("checkpoint_id: {}", snapshot.checkpoint_id);
        }
    }

    Ok(())
}

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn atomic_write_bytes(output: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    atomic_write_with(output, |file| {
        file.write_all(bytes)?;
        Ok(())
    })
}

fn atomic_write_with(
    output: impl AsRef<Path>,
    operation: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
    let output = output.as_ref();
    let (temporary_path, mut temporary) = create_temporary_sibling(output)?;
    let result = operation(&mut temporary)
        .and_then(|()| {
            temporary
                .flush()
                .context("failed to flush temporary output")
        })
        .and_then(|()| {
            temporary
                .sync_all()
                .context("failed to sync temporary output")
        });
    drop(temporary);

    if let Err(error) = result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary_path, output) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error).with_context(|| format!("failed to replace {}", output.display()));
    }
    Ok(())
}

fn create_temporary_sibling(output: &Path) -> Result<(PathBuf, fs::File)> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("compact-output");

    for _ in 0..32 {
        let id = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), id));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to create {}", path.display()));
            }
        }
    }

    anyhow::bail!("failed to allocate a unique temporary output path")
}

fn write_new_file(output: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    let output = output.as_ref();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .with_context(|| {
            format!(
                "output already exists or cannot be created: {}",
                output.display()
            )
        })?;
    if let Err(error) = file
        .write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(output);
        return Err(error).with_context(|| format!("failed to write {}", output.display()));
    }
    Ok(())
}

fn inspect_file(data: &[u8]) -> Result<()> {
    if data.len() >= 4 && data[0..4] == compact_core::MAGIC_V4 {
        inspect_v4_file(data)
    } else if data.len() >= 4 && data[0..4] == compact_core::MAGIC_V3 {
        inspect_v3_file(data)
    } else if data.len() >= 4 && data[0..4] == compact_core::MAGIC_V2 {
        inspect_stream_file(data)
    } else {
        inspect_v1_frame(data)
    }
}

fn inspect_v4_file(data: &[u8]) -> Result<()> {
    let footer = compact_core::io::v4::inspect_footer(data).context("failed to inspect CMP4")?;
    println!("version: {}", compact_core::VERSION_V4);
    println!("format: cmp4");
    println!("row_groups: {}", footer.row_groups.len());
    println!("rows: {}", footer.total_row_count);
    for row_group in footer.row_groups {
        println!(
            "row_group {} first_row={} rows={} offset={} len={} columns={}",
            row_group.row_group_index,
            row_group.first_row_index,
            row_group.row_count,
            row_group.row_group_offset,
            row_group.row_group_len,
            row_group.columns.len()
        );
        for column in row_group.columns {
            let statistics = compact_core::statistics::decode(&column.statistics_metadata)
                .context("failed to decode CMP4 column statistics")?;
            println!(
                "column row_group={} name={} rows={} nulls={} metadata_offset={} metadata_len={} payload_offset={} payload_len={} stats={:?}",
                row_group.row_group_index,
                column.name,
                column.value_count,
                column.null_count,
                column.metadata_offset,
                column.metadata_len,
                column.payload_offset,
                column.payload_len,
                statistics
            );
        }
    }

    Ok(())
}

fn inspect_v3_file(data: &[u8]) -> Result<()> {
    let inspect = compact_core::io::v3::inspect_jsonl(data).context("failed to inspect CMP3")?;
    println!("version: {}", compact_core::VERSION_V3);
    println!("format: cmp3");
    println!("rows: {}", inspect.row_count);
    println!("raw_bytes: {}", inspect.raw_size);
    println!("encoded_bytes: {}", inspect.encoded_size);
    for column in inspect.columns {
        println!(
            "column name={} type={:?} codec={:?} rows={} nulls={} raw={} compressed={} stats={:?}",
            column.metadata.name,
            column.metadata.value_type,
            column.metadata.codec,
            column.metadata.value_count,
            column.metadata.null_count,
            column.metadata.raw_size,
            column.metadata.compressed_size,
            column.statistics
        );
    }
    Ok(())
}

fn inspect_stream_file(data: &[u8]) -> Result<()> {
    let inspect = compact_core::streaming::inspect_stream(Cursor::new(data))
        .context("failed to inspect stream")?;

    println!("version: {}", compact_core::VERSION_V2);
    println!("format: stream");
    println!("blocks: {}", inspect.blocks.len());
    if let Some(index) = &inspect.footer_index {
        println!("index: footer");
        println!("index_blocks: {}", index.len());
    } else {
        println!("index: scan");
    }
    println!("total_rows: {}", inspect.total_rows);
    println!("total_raw_bytes: {}", inspect.total_uncompressed_size);
    println!("total_compressed_bytes: {}", inspect.total_compressed_size);
    print_ratio(
        inspect.total_uncompressed_size,
        inspect.total_compressed_size,
    );

    for block in inspect.blocks {
        println!(
            "block {} offset={} rows={} raw={} compressed={} checksum={:08x}",
            block.block_index,
            block.encoded_offset,
            block.row_count,
            block.uncompressed_size,
            block.compressed_size,
            block.checksum
        );
    }

    Ok(())
}

fn inspect_append_recovery(data: &[u8]) -> Result<()> {
    let recovery = compact_core::streaming::recover_append_stream(data)
        .context("failed to recover append stream")?;
    println!("format: append-stream");
    println!("valid_len: {}", recovery.valid_len);
    println!("blocks: {}", recovery.blocks.len());
    println!("total_rows: {}", recovery.total_rows);
    println!("total_raw_bytes: {}", recovery.total_uncompressed_size);
    println!("total_compressed_bytes: {}", recovery.total_compressed_size);
    println!(
        "truncated_or_corrupt_tail: {}",
        recovery.truncated_or_corrupt_tail
    );
    for block in recovery.blocks {
        println!(
            "block {} offset={} rows={} raw={} compressed={} checksum={:08x}",
            block.block_index,
            block.encoded_offset,
            block.row_count,
            block.uncompressed_size,
            block.compressed_size,
            block.checksum
        );
    }

    Ok(())
}

fn inspect_v1_frame(frame: &[u8]) -> Result<()> {
    let decoded = compact_core::framing::decode_v1(frame).context("failed to inspect frame")?;

    println!("version: {}", compact_core::VERSION_V1);
    println!("format: frame");
    println!("codec: {:?}", decoded.codec);
    println!("payload_len: {}", decoded.payload.len());

    if decoded.codec == Codec::ColumnBlock {
        let inspect =
            compact_core::io::inspect_jsonl(frame).context("failed to inspect columns")?;
        println!("columns: {}", inspect.columns.len());
        for column in inspect.columns {
            println!(
                "column name={} codec={:?} rows={} payload_len={}",
                column.name, column.codec, column.row_count, column.payload_len
            );
        }
    }

    Ok(())
}

impl CliCodec {
    fn byte_config(self) -> EncodeConfig {
        match self {
            CliCodec::Rle => EncodeConfig {
                value_type: ValueType::RawBytes,
                transform: Transform::None,
                codec: Codec::Rle,
            },
        }
    }
}

fn load_schema(path: &str) -> Result<compact_core::schema::Schema> {
    let data = fs::read_to_string(path).with_context(|| format!("failed to read {path}"))?;

    compact_core::schema::Schema::from_yaml(&data).context("failed to parse schema")
}

fn load_schema_revision(path: &str) -> Result<compact_core::schema::evolution::SchemaRevision> {
    let data = fs::read_to_string(path).with_context(|| format!("failed to read {path}"))?;

    compact_core::schema::evolution::SchemaRevision::from_yaml(&data)
        .context("failed to parse schema revision")
}

fn parse_migration_assignments(
    values: &[String],
) -> Result<compact_core::storage::migration::MigrationAssignments> {
    let mut stable_ids = std::collections::BTreeMap::new();
    for value in values {
        let (name, stable_id) = value
            .split_once('=')
            .context("--column-id must use name=positive_integer")?;
        if name.is_empty() {
            anyhow::bail!("--column-id name must not be empty");
        }
        let stable_id = stable_id
            .parse::<u32>()
            .context("--column-id must use name=positive_integer")?;
        if stable_ids.insert(name.to_owned(), stable_id).is_some() {
            anyhow::bail!("--column-id contains a duplicate column name");
        }
    }

    Ok(compact_core::storage::migration::MigrationAssignments { stable_ids })
}

fn block_options(
    block_rows: Option<usize>,
    block_bytes: Option<usize>,
) -> compact_core::Result<compact_core::streaming::BlockOptions> {
    let defaults = compact_core::streaming::BlockOptions::default();

    compact_core::streaming::BlockOptions {
        max_rows_per_block: block_rows.unwrap_or(defaults.max_rows_per_block),
        max_uncompressed_bytes_per_block: block_bytes
            .unwrap_or(defaults.max_uncompressed_bytes_per_block),
    }
    .validate()
}

fn cmp4_options(
    block_rows: Option<usize>,
    block_bytes: Option<usize>,
) -> Result<compact_core::io::v4::EncodeOptions> {
    if block_bytes.is_some() {
        anyhow::bail!("CMP4 currently supports --block-rows but not --block-bytes");
    }

    Ok(compact_core::io::v4::EncodeOptions {
        row_group_rows: block_rows
            .unwrap_or_else(|| compact_core::io::v4::EncodeOptions::default().row_group_rows),
    })
}

fn parse_projection(project: Option<&str>) -> Vec<String> {
    project
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_predicate(
    filter_column: Option<String>,
    filter_op: Option<CliFilterOp>,
    filter_value: Option<u64>,
) -> Result<Option<compact_core::io::v4::Predicate>> {
    match (filter_column, filter_op, filter_value) {
        (None, None, None) => Ok(None),
        (Some(column), Some(CliFilterOp::IsNull), None) => {
            Ok(Some(compact_core::io::v4::Predicate::IsNull { column }))
        }
        (Some(column), Some(op), Some(value)) => {
            let op = match op {
                CliFilterOp::Eq => compact_core::io::v4::U64PredicateOp::Eq(value),
                CliFilterOp::Lt => compact_core::io::v4::U64PredicateOp::Lt(value),
                CliFilterOp::Le => compact_core::io::v4::U64PredicateOp::Le(value),
                CliFilterOp::Gt => compact_core::io::v4::U64PredicateOp::Gt(value),
                CliFilterOp::Ge => compact_core::io::v4::U64PredicateOp::Ge(value),
                CliFilterOp::IsNull => anyhow::bail!("is-null filter must not include a value"),
            };
            Ok(Some(compact_core::io::v4::Predicate::U64 { column, op }))
        }
        _ => anyhow::bail!(
            "filter requires --filter-column and --filter-op; u64 filters also require --filter-value"
        ),
    }
}

fn parse_search_postings(
    input: &str,
) -> Result<Vec<compact_core::search::dictionary::TermPostingList>> {
    let mut terms =
        std::collections::BTreeMap::<String, Vec<compact_core::search::postings::Posting>>::new();

    for (line_index, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut fields = line.split_whitespace();
        let term = fields
            .next()
            .with_context(|| format!("line {} is missing term", line_index + 1))?;
        let doc_id = fields
            .next()
            .with_context(|| format!("line {} is missing doc_id", line_index + 1))?
            .parse::<u64>()
            .with_context(|| format!("line {} has invalid doc_id", line_index + 1))?;
        let positions = fields
            .next()
            .map(parse_positions)
            .transpose()
            .with_context(|| format!("line {} has invalid positions", line_index + 1))?
            .unwrap_or_default();

        if fields.next().is_some() {
            anyhow::bail!("line {} has too many fields", line_index + 1);
        }

        terms
            .entry(term.to_owned())
            .or_default()
            .push(compact_core::search::postings::Posting { doc_id, positions });
    }

    let entries = terms
        .into_iter()
        .map(|(term, mut postings)| {
            postings.sort_by_key(|posting| posting.doc_id);
            compact_core::search::dictionary::TermPostingList { term, postings }
        })
        .collect::<Vec<_>>();

    Ok(entries)
}

fn parse_positions(input: &str) -> Result<Vec<u64>> {
    if input == "-" {
        return Ok(Vec::new());
    }

    input
        .split(',')
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("invalid position {value}"))
        })
        .collect()
}

fn inspect_search_dictionary(data: &[u8]) -> Result<()> {
    let index = compact_core::search::dictionary::inspect_dictionary(data)
        .context("failed to inspect search dictionary")?;
    println!("format: search");
    println!("terms: {}", index.term_count);
    println!("postings_bytes: {}", index.postings_bytes);

    for entry in index.entries {
        println!(
            "term {} docs={} postings_offset={} postings_len={}",
            entry.term, entry.doc_count, entry.postings_offset, entry.postings_len
        );
    }

    Ok(())
}

fn join_u64s(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn print_ratio(input_len: u64, output_len: u64) {
    println!("input_bytes: {input_len}");
    println!("output_bytes: {output_len}");
    println!(
        "compression_ratio: {:.4}",
        compression_ratio(input_len, output_len)
    );
}

fn compression_ratio(input_len: u64, output_len: u64) -> f64 {
    if input_len == 0 {
        1.0
    } else {
        output_len as f64 / input_len as f64
    }
}

fn mib_per_second(bytes: u64, seconds: f64) -> f64 {
    if seconds == 0.0 {
        return 0.0;
    }

    (bytes as f64 / 1024.0 / 1024.0) / seconds
}
