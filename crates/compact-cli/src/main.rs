use std::fs;
use std::io::{BufReader, Cursor};
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
    },
    Decode {
        input: String,
        output: String,
        #[arg(long, value_enum, default_value_t = CliCodec::Rle)]
        codec: CliCodec,
        #[arg(long)]
        schema: Option<String>,
    },
    Inspect {
        input: String,
    },
    Bench {
        input: String,
        #[arg(long)]
        schema: String,
        #[arg(long)]
        block_rows: Option<usize>,
        #[arg(long)]
        block_bytes: Option<usize>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliCodec {
    Rle,
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
        } => {
            if let Some(schema_path) = schema {
                let schema = load_schema(&schema_path)?;
                let options = block_options(block_rows, block_bytes)
                    .context("invalid streaming block options")?;
                let input_file =
                    fs::File::open(&input).with_context(|| format!("failed to open {input}"))?;
                let output_file = fs::File::create(&output)
                    .with_context(|| format!("failed to create {output}"))?;

                compact_core::streaming::encode_jsonl_stream(
                    BufReader::new(input_file),
                    output_file,
                    schema,
                    options,
                )
                .context("failed to stream encode JSONL input")?;
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
        } => {
            if let Some(schema_path) = schema {
                let schema = load_schema(&schema_path)?;
                let input_file =
                    fs::File::open(&input).with_context(|| format!("failed to open {input}"))?;
                let output_file = fs::File::create(&output)
                    .with_context(|| format!("failed to create {output}"))?;

                compact_core::streaming::decode_jsonl_stream(input_file, output_file, schema)
                    .context("failed to stream decode JSONL input")?;
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
        Commands::Bench {
            input,
            schema,
            block_rows,
            block_bytes,
        } => {
            let schema = load_schema(&schema)?;
            let options = block_options(block_rows, block_bytes)
                .context("invalid streaming block options")?;
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
            let input_bytes = input_text.len() as u64;
            let encode_start = Instant::now();
            let encoded = compact_core::streaming::encode_jsonl_stream(
                BufReader::new(Cursor::new(input_text.as_bytes())),
                Vec::new(),
                schema.clone(),
                options,
            )
            .context("failed to stream encode JSONL benchmark input")?;
            let encode_elapsed = encode_start.elapsed();
            let inspect = compact_core::streaming::inspect_stream(Cursor::new(&encoded))
                .context("failed to inspect benchmark stream")?;
            let decode_start = Instant::now();
            let decoded = compact_core::streaming::decode_jsonl_stream(
                Cursor::new(&encoded),
                Vec::new(),
                schema,
            )
            .context("failed to stream decode JSONL benchmark input")?;
            let decode_elapsed = decode_start.elapsed();

            if decoded != input_text.as_bytes() {
                anyhow::bail!("benchmark roundtrip mismatch");
            }

            println!("mode: stream");
            println!("block_rows: {}", options.max_rows_per_block);
            println!("block_bytes: {}", options.max_uncompressed_bytes_per_block);
            println!("blocks: {}", inspect.blocks.len());
            println!("rows: {}", inspect.total_rows);
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
    }

    Ok(())
}

fn inspect_file(data: &[u8]) -> Result<()> {
    if data.len() >= 4 && data[0..4] == compact_core::MAGIC_V2 {
        inspect_stream_file(data)
    } else {
        inspect_v1_frame(data)
    }
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
