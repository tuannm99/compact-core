use std::fs;

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
        }
        Commands::Bench { input, schema } => {
            println!(
                "compact {}: bench is not implemented yet (input={input}, schema={schema})",
                compact_core::crate_version()
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
