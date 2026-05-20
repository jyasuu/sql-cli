use clap::{Parser, Subcommand, ValueEnum};
use sqlparser::dialect::*;
use sqlparser::parser::Parser as SqlParser;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "sql-cli",
    version,
    about = "Minimal SQL parser CLI powered by datafusion-sqlparser-rs",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse SQL and pretty-print it back (format / normalise)
    // FUTURE: add --indent, --uppercase-keywords flags
    Format {
        /// SQL string to format (reads stdin if omitted)
        sql: Option<String>,

        /// SQL dialect
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
    },

    /// Parse SQL and emit the AST as JSON
    // FUTURE: add --compact flag, --path <jsonpath> filter
    Ast {
        /// SQL string to parse (reads stdin if omitted)
        sql: Option<String>,

        /// SQL dialect
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
    },

    /// Validate SQL syntax — exit 0 if valid, 1 if not
    // FUTURE: add --quiet flag, batch-file support
    Validate {
        /// SQL string to validate (reads stdin if omitted)
        sql: Option<String>,

        /// SQL dialect
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
    },
    // FUTURE: `repl` subcommand — interactive session with readline + AST inspection
}

// ---------------------------------------------------------------------------
// Supported dialects (thin wrapper so clap can parse them)
// ---------------------------------------------------------------------------

#[derive(Clone, ValueEnum)]
enum Dialect {
    Generic,
    Ansi,
    BigQuery,
    ClickHouse,
    Hive,
    MsSql,
    MySql,
    PostgreSql,
    Redshift,
    Snowflake,
    Sqlite,
    DuckDb,
}

impl Dialect {
    fn as_boxed(&self) -> Box<dyn sqlparser::dialect::Dialect> {
        match self {
            Dialect::Generic    => Box::new(GenericDialect {}),
            Dialect::Ansi       => Box::new(AnsiDialect {}),
            Dialect::BigQuery   => Box::new(BigQueryDialect {}),
            Dialect::ClickHouse => Box::new(ClickHouseDialect {}),
            Dialect::Hive       => Box::new(HiveDialect {}),
            Dialect::MsSql      => Box::new(MsSqlDialect {}),
            Dialect::MySql      => Box::new(MySqlDialect {}),
            Dialect::PostgreSql => Box::new(PostgreSqlDialect {}),
            Dialect::Redshift   => Box::new(RedshiftSqlDialect {}),
            Dialect::Snowflake  => Box::new(SnowflakeDialect {}),
            Dialect::Sqlite     => Box::new(SQLiteDialect {}),
            Dialect::DuckDb     => Box::new(DuckDbDialect {}),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read from the provided string or fall back to stdin.
fn read_sql(opt: Option<String>) -> String {
    match opt {
        Some(s) => s,
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).expect("failed to read stdin");
            buf
        }
    }
}

/// Parse SQL with the chosen dialect, returning a human-friendly error on failure.
fn parse(sql: &str, dialect: &Dialect) -> Result<Vec<sqlparser::ast::Statement>, String> {
    SqlParser::parse_sql(dialect.as_boxed().as_ref(), sql)
        .map_err(|e| format!("Parse error: {e}"))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    match cli.command {
        // ── format ─────────────────────────────────────────────────────────
        Command::Format { sql, dialect } => {
            let sql = read_sql(sql);
            match parse(&sql, &dialect) {
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                Ok(stmts) => {
                    for stmt in &stmts {
                        println!("{stmt}");
                    }
                }
            }
        }

        // ── ast ────────────────────────────────────────────────────────────
        Command::Ast { sql, dialect } => {
            let sql = read_sql(sql);
            match parse(&sql, &dialect) {
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                Ok(stmts) => {
                    // sqlparser AST types implement Serialize via the serde feature
                    match serde_json::to_string_pretty(&stmts) {
                        Ok(json) => println!("{json}"),
                        Err(e)   => {
                            eprintln!("JSON serialisation error: {e}");
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        // ── validate ───────────────────────────────────────────────────────
        Command::Validate { sql, dialect } => {
            let sql = read_sql(sql);
            match parse(&sql, &dialect) {
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                Ok(stmts) => {
                    let count = stmts.len();
                    println!("OK — {count} statement{}", if count == 1 { "" } else { "s" });
                }
            }
        }
    }
}
