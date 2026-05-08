use clap::{Parser, Subcommand};

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
        #[arg(long)]
        schema: String,
    },
    Decode {
        input: String,
        output: String,
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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Encode {
            input,
            output,
            schema,
        } => {
            println!(
                "compact {}: encode is not implemented yet (input={input}, schema={schema}, output={output})",
                compact_core::crate_version()
            );
        }
        Commands::Decode { input, output } => {
            println!(
                "compact {}: decode is not implemented yet (input={input}, output={output})",
                compact_core::crate_version()
            );
        }
        Commands::Inspect { input } => {
            println!(
                "compact {}: inspect is not implemented yet (input={input})",
                compact_core::crate_version()
            );
        }
        Commands::Bench { input, schema } => {
            println!(
                "compact {}: bench is not implemented yet (input={input}, schema={schema})",
                compact_core::crate_version()
            );
        }
    }
}
