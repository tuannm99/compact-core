use std::fs;
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
        } => {
            let encoded = if let Some(schema_path) = schema {
                let schema = load_schema(&schema_path)?;
                let input_text = fs::read_to_string(&input)
                    .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;

                compact_core::io::encode_jsonl(&input_text, &schema)
                    .context("failed to encode JSONL input")?
            } else {
                let config = codec.byte_config();
                let input_bytes =
                    fs::read(&input).with_context(|| format!("failed to read {input}"))?;

                compact_core::encode_bytes_frame(&config, &input_bytes)
                    .context("failed to encode input")?
            };

            fs::write(&output, encoded).with_context(|| format!("failed to write {output}"))?;
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
            let frame = fs::read(&input).with_context(|| format!("failed to read {input}"))?;

            if let Some(schema_path) = schema {
                let schema = load_schema(&schema_path)?;
                let decoded = compact_core::io::decode_jsonl(&frame, &schema)
                    .context("failed to decode JSONL frame")?;

                fs::write(&output, decoded).with_context(|| format!("failed to write {output}"))?;
            } else {
                let config = codec.byte_config();
                let decoded = compact_core::decode_bytes_frame(&config, &frame)
                    .context("failed to decode input")?;

                fs::write(&output, decoded).with_context(|| format!("failed to write {output}"))?;
            }
        }
        Commands::Inspect { input } => {
            let frame = fs::read(&input).with_context(|| format!("failed to read {input}"))?;
            let decoded =
                compact_core::framing::decode_v1(&frame).context("failed to inspect frame")?;

            println!("version: {}", compact_core::VERSION_V1);
            println!("codec: {:?}", decoded.codec);
            println!("payload_len: {}", decoded.payload.len());

            if decoded.codec == Codec::ColumnBlock {
                let inspect =
                    compact_core::io::inspect_jsonl(&frame).context("failed to inspect columns")?;
                println!("columns: {}", inspect.columns.len());
                for column in inspect.columns {
                    println!(
                        "column name={} codec={:?} rows={} payload_len={}",
                        column.name, column.codec, column.row_count, column.payload_len
                    );
                }
            }
        }
        Commands::Bench { input, schema } => {
            let schema = load_schema(&schema)?;
            let input_text = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {input} as UTF-8 JSONL"))?;
            let encode_start = Instant::now();
            let encoded = compact_core::io::encode_jsonl(&input_text, &schema)
                .context("failed to encode JSONL input")?;
            let encode_elapsed = encode_start.elapsed();
            let decode_start = Instant::now();
            let decoded = compact_core::io::decode_jsonl(&encoded, &schema)
                .context("failed to decode JSONL frame")?;
            let decode_elapsed = decode_start.elapsed();

            if decoded != input_text {
                anyhow::bail!("benchmark roundtrip mismatch");
            }

            println!("input_bytes: {}", input_text.len());
            println!("encoded_bytes: {}", encoded.len());
            print_ratio(input_text.len() as u64, encoded.len() as u64);
            println!("encode_ms: {:.3}", encode_elapsed.as_secs_f64() * 1000.0);
            println!("decode_ms: {:.3}", decode_elapsed.as_secs_f64() * 1000.0);
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

fn print_ratio(input_len: u64, output_len: u64) {
    let ratio = if input_len == 0 {
        1.0
    } else {
        output_len as f64 / input_len as f64
    };
    println!("input_bytes: {input_len}");
    println!("output_bytes: {output_len}");
    println!("compression_ratio: {ratio:.4}");
}
