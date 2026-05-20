use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use sqlparser::ast::Statement;
use sqlparser::dialect::*;
use sqlparser::parser::Parser as SqlParser;

// ─────────────────────────────────────────────────────────────────────────────
// CLI definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "sql-cli",
    version,
    about = "SQL parser CLI powered by datafusion-sqlparser-rs",
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse SQL and pretty-print it (format / normalise)
    Format {
        /// SQL string — reads stdin or --file if omitted
        sql: Option<String>,
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
        /// Read SQL from a file instead of a literal argument
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Parse SQL and emit the AST
    Ast {
        /// SQL string — reads stdin or --file if omitted
        sql: Option<String>,
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
        /// Read SQL from a file instead of a literal argument
        #[arg(short, long)]
        file: Option<String>,
        /// Output format
        #[arg(short, long, default_value = "json-pretty")]
        output: OutputFormat,
    },

    /// Validate SQL syntax — exit 0 if valid, 1 otherwise
    Validate {
        /// SQL string — reads stdin or --file if omitted
        sql: Option<String>,
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
        /// Read SQL from a file instead of a literal argument
        #[arg(short, long)]
        file: Option<String>,
        /// Suppress success output (only print errors)
        #[arg(short, long)]
        quiet: bool,
    },

    /// Diff the ASTs of two SQL strings
    Diff {
        /// First SQL string (or path with --file1)
        sql1: Option<String>,
        /// Second SQL string (or path with --file2)
        sql2: Option<String>,
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
        #[arg(long)]
        file1: Option<String>,
        #[arg(long)]
        file2: Option<String>,
    },

    /// Start an interactive REPL
    Repl {
        #[arg(short, long, default_value = "generic")]
        dialect: Dialect,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Output format
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    /// Indented JSON (default)
    JsonPretty,
    /// Compact single-line JSON
    Json,
    /// YAML
    Yaml,
    /// Human-readable tree
    Tree,
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialects
// ─────────────────────────────────────────────────────────────────────────────

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
    fn name(&self) -> &'static str {
        match self {
            Dialect::Generic    => "generic",
            Dialect::Ansi       => "ansi",
            Dialect::BigQuery   => "bigquery",
            Dialect::ClickHouse => "clickhouse",
            Dialect::Hive       => "hive",
            Dialect::MsSql      => "mssql",
            Dialect::MySql      => "mysql",
            Dialect::PostgreSql => "postgresql",
            Dialect::Redshift   => "redshift",
            Dialect::Snowflake  => "snowflake",
            Dialect::Sqlite     => "sqlite",
            Dialect::DuckDb     => "duckdb",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve SQL input: explicit string → file → stdin (in that order).
fn resolve_sql(sql: Option<String>, file: Option<String>) -> Result<String, String> {
    if let Some(s) = sql {
        return Ok(s);
    }
    if let Some(path) = file {
        return std::fs::read_to_string(&path)
            .map_err(|e| format!("Cannot read '{}': {e}", path));
    }
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("stdin error: {e}"))?;
    Ok(buf)
}

/// Parse (supports multiple `;`-separated statements).
fn parse(sql: &str, dialect: &Dialect) -> Result<Vec<Statement>, String> {
    SqlParser::parse_sql(dialect.as_boxed().as_ref(), sql)
        .map_err(|e| format!("{}", e))
}

/// Render AST in the requested output format.
fn render_ast(stmts: &[Statement], fmt: &OutputFormat) -> Result<String, String> {
    match fmt {
        OutputFormat::JsonPretty =>
            serde_json::to_string_pretty(stmts).map_err(|e| e.to_string()),
        OutputFormat::Json =>
            serde_json::to_string(stmts).map_err(|e| e.to_string()),
        OutputFormat::Yaml =>
            serde_yaml::to_string(stmts).map_err(|e| format!("YAML output not supported for this query (sqlparser AST uses nested enums unsupported by serde_yaml): {e}")),
        OutputFormat::Tree =>
            Ok(render_tree(stmts)),
    }
}

/// Simple recursive tree renderer using the JSON value as a proxy.
fn render_tree(stmts: &[Statement]) -> String {
    let val = serde_json::to_value(stmts).unwrap_or(serde_json::Value::Null);
    let mut out = String::new();
    render_value(&val, &mut out, "", true);
    out
}

fn render_value(val: &serde_json::Value, out: &mut String, prefix: &str, last: bool) {
    let connector = if last { "└── " } else { "├── " };
    let child_prefix = if last { "    " } else { "│   " };

    match val {
        serde_json::Value::Object(map) => {
            for (i, (k, v)) in map.iter().enumerate() {
                let is_last = i == map.len() - 1;
                match v {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        out.push_str(&format!(
                            "{}{}{}\n",
                            prefix,
                            if i == 0 { connector } else { if last { "    " } else { "│   " } },
                            k.cyan().bold()
                        ));
                        render_value(v, out, &format!("{}{}", prefix, child_prefix), is_last);
                    }
                    serde_json::Value::Null => {}
                    _ => {
                        out.push_str(&format!(
                            "{}{}{}: {}\n",
                            prefix,
                            if i == 0 { connector } else { if last { "    " } else { "│   " } },
                            k.cyan(),
                            format!("{}", v).green()
                        ));
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                let is_last = i == arr.len() - 1;
                let idx = format!("[{}]", i).yellow().to_string();
                match item {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        out.push_str(&format!("{}{}{}\n", prefix, connector, idx));
                        render_value(item, out, &format!("{}{}", prefix, child_prefix), is_last);
                    }
                    _ => {
                        out.push_str(&format!("{}{}{}: {}\n", prefix, connector, idx, format!("{}", item).green()));
                    }
                }
            }
        }
        other => {
            out.push_str(&format!("{}{}{}\n", prefix, connector, format!("{}", other).green()));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Diff helpers
// ─────────────────────────────────────────────────────────────────────────────

fn diff_sql(sql1: &str, sql2: &str, dialect: &Dialect) -> i32 {
    let stmts1 = match parse(sql1, dialect) {
        Ok(s) => s,
        Err(e) => { eprintln!("{} {e}", "Error (sql1):".red().bold()); return 1; }
    };
    let stmts2 = match parse(sql2, dialect) {
        Ok(s) => s,
        Err(e) => { eprintln!("{} {e}", "Error (sql2):".red().bold()); return 1; }
    };

    // Compare normalised formatted SQL (whitespace-insensitive)
    let fmt1 = stmts1.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(";\n");
    let fmt2 = stmts2.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(";\n");

    if fmt1 == fmt2 {
        println!("{}", "✓ Statements are semantically identical.".green().bold());
        return 0;
    }

    // Line-level diff
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(&fmt1, &fmt2);
    println!("{}", "─── SQL diff (formatted) ───".bold());
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => print!("{}", format!("- {}", change).red()),
            ChangeTag::Insert => print!("{}", format!("+ {}", change).green()),
            ChangeTag::Equal  => print!("  {}", change),
        }
    }

    // AST-level diff (JSON)
    let ast1 = serde_json::to_string_pretty(&stmts1).unwrap_or_default();
    let ast2 = serde_json::to_string_pretty(&stmts2).unwrap_or_default();
    if ast1 != ast2 {
        println!("\n{}", "─── AST diff (JSON) ───".bold());
        let adiff = TextDiff::from_lines(&ast1, &ast2);
        for change in adiff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Delete => print!("{}", format!("- {}", change).red()),
                ChangeTag::Insert => print!("{}", format!("+ {}", change).green()),
                ChangeTag::Equal  => {}  // suppress equal AST lines for brevity
            }
        }
    }

    1
}

// ─────────────────────────────────────────────────────────────────────────────
// REPL
// ─────────────────────────────────────────────────────────────────────────────

fn run_repl(dialect: &Dialect) {
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;

    println!(
        "{}\nDialect: {}  |  {}",
        "sql-cli REPL — datafusion-sqlparser-rs".bold().cyan(),
        dialect.name().yellow(),
        "Commands: :ast  :ast-yaml  :ast-tree  :fmt  :validate  :dialect <d>  :help  :quit".dimmed()
    );
    println!("{}", "Terminate multi-line input with a blank line or ';'.".dimmed());

    let mut rl = DefaultEditor::new().expect("failed to init readline");
    let mut current_dialect = dialect.clone();

    loop {
        let readline = rl.readline(&format!("{} ", "sql>".cyan().bold()));
        match readline {
            Ok(line) => {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() { continue; }
                let _ = rl.add_history_entry(&trimmed);

                // ── REPL meta-commands ──────────────────────────────────────
                if let Some(cmd) = trimmed.strip_prefix(':') {
                    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                    match parts[0] {
                        "quit" | "q" | "exit" => {
                            println!("Bye!");
                            return;
                        }
                        "help" | "h" => {
                            print_repl_help();
                            continue;
                        }
                        "dialect" => {
                            if let Some(d) = parts.get(1) {
                                match parse_dialect_str(d.trim()) {
                                    Some(nd) => {
                                        current_dialect = nd;
                                        println!("Dialect set to {}", d.trim().yellow());
                                    }
                                    None => eprintln!("{}", format!("Unknown dialect '{}'", d.trim()).red()),
                                }
                            } else {
                                println!("Current dialect: {}", current_dialect.name().yellow());
                            }
                            continue;
                        }
                        _ => {
                            // Command with a SQL body: collect until ; or blank line
                            let meta_cmd = parts[0].to_string();
                            let inline_sql = parts.get(1).map(|s| s.trim().to_string());
                            let sql = if let Some(s) = inline_sql.filter(|s| !s.is_empty()) {
                                s
                            } else {
                                read_multiline_sql(&mut rl)
                            };
                            dispatch_repl_command(&meta_cmd, &sql, &current_dialect);
                            continue;
                        }
                    }
                }

                // ── Plain SQL → default: format + validate ──────────────────
                match parse(&trimmed, &current_dialect) {
                    Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
                    Ok(stmts) => {
                        for (i, stmt) in stmts.iter().enumerate() {
                            if stmts.len() > 1 {
                                println!("{}", format!("── Statement {} ──", i + 1).dimmed());
                            }
                            println!("{}", stmt.to_string().green());
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("Bye!");
                break;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                break;
            }
        }
    }
}

fn read_multiline_sql(rl: &mut rustyline::DefaultEditor) -> String {
    let mut lines = Vec::new();
    loop {
        match rl.readline("  ... ") {
            Ok(l) => {
                let t = l.trim_end().to_string();
                if t.is_empty() || t.ends_with(';') {
                    if !t.is_empty() { lines.push(t); }
                    break;
                }
                lines.push(t);
            }
            _ => break,
        }
    }
    lines.join(" ")
}

fn dispatch_repl_command(cmd: &str, sql: &str, dialect: &Dialect) {
    match cmd {
        "ast" => {
            match parse(sql, dialect) {
                Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
                Ok(stmts) => match render_ast(&stmts, &OutputFormat::JsonPretty) {
                    Ok(s) => println!("{s}"),
                    Err(e) => eprintln!("{e}"),
                },
            }
        }
        "ast-yaml" => {
            match parse(sql, dialect) {
                Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
                Ok(stmts) => match render_ast(&stmts, &OutputFormat::Yaml) {
                    Ok(s) => println!("{s}"),
                    Err(e) => eprintln!("{e}"),
                },
            }
        }
        "ast-tree" => {
            match parse(sql, dialect) {
                Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
                Ok(stmts) => println!("{}", render_ast(&stmts, &OutputFormat::Tree).unwrap_or_default()),
            }
        }
        "fmt" | "format" => {
            match parse(sql, dialect) {
                Err(e) => eprintln!("{} {e}", "Error:".red().bold()),
                Ok(stmts) => {
                    for stmt in &stmts { println!("{}", stmt.to_string().green()); }
                }
            }
        }
        "validate" => {
            match parse(sql, dialect) {
                Err(e) => eprintln!("{} {e}", "✗".red().bold()),
                Ok(stmts) => println!(
                    "{} {} statement{}",
                    "✓".green().bold(),
                    stmts.len(),
                    if stmts.len() == 1 { "" } else { "s" }
                ),
            }
        }
        other => eprintln!("{}", format!("Unknown command ':{other}'. Type :help for help.").yellow()),
    }
}

fn print_repl_help() {
    println!(
        r#"
{header}

  {plain}         — parse & format SQL (default)
  {ast}           — show AST as indented JSON
  {yaml}     — show AST as YAML
  {tree}     — show AST as a visual tree
  {fmt}           — re-format / normalise SQL
  {validate}  — check syntax only
  {dialect}  — show or change dialect
  {help}          — show this help
  {quit}          — exit the REPL

  Multi-line SQL: type SQL across lines; end with ';' or a blank line.
"#,
        header   = "REPL commands".bold().underline(),
        plain    = "<sql>".cyan(),
        ast      = ":ast <sql>".cyan(),
        yaml     = ":ast-yaml <sql>".cyan(),
        tree     = ":ast-tree <sql>".cyan(),
        fmt      = ":fmt <sql>".cyan(),
        validate = ":validate <sql>".cyan(),
        dialect  = ":dialect [name]".cyan(),
        help     = ":help".cyan(),
        quit     = ":quit".cyan(),
    );
}

fn parse_dialect_str(s: &str) -> Option<Dialect> {
    match s.to_lowercase().as_str() {
        "generic"    => Some(Dialect::Generic),
        "ansi"       => Some(Dialect::Ansi),
        "bigquery"   => Some(Dialect::BigQuery),
        "clickhouse" => Some(Dialect::ClickHouse),
        "hive"       => Some(Dialect::Hive),
        "mssql"      => Some(Dialect::MsSql),
        "mysql"      => Some(Dialect::MySql),
        "postgresql" | "postgres" => Some(Dialect::PostgreSql),
        "redshift"   => Some(Dialect::Redshift),
        "snowflake"  => Some(Dialect::Snowflake),
        "sqlite"     => Some(Dialect::Sqlite),
        "duckdb"     => Some(Dialect::DuckDb),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch validation helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Validate all `;`-separated statements and report per-statement results.
fn validate_batch(sql: &str, dialect: &Dialect, quiet: bool) -> i32 {
    // Split on `;` preserving content
    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if statements.is_empty() {
        if !quiet { println!("{}", "No statements found.".yellow()); }
        return 0;
    }

    let mut errors = 0;
    for (i, stmt) in statements.iter().enumerate() {
        let n = i + 1;
        let full = format!("{};", stmt);
        match parse(&full, dialect) {
            Ok(_) => {
                if !quiet {
                    println!("{} Statement {n}: {}", "✓".green().bold(), stmt.chars().take(60).collect::<String>().dimmed());
                }
            }
            Err(e) => {
                errors += 1;
                eprintln!("{} Statement {n}: {}\n     {}", "✗".red().bold(), stmt.chars().take(60).collect::<String>(), e.red());
            }
        }
    }

    if !quiet {
        let total = statements.len();
        let ok = total - errors;
        println!(
            "\n{} {ok}/{total} statement{} valid",
            if errors == 0 { "✓".green().bold() } else { "✗".red().bold() },
            if total == 1 { "" } else { "s" }
        );
    }

    if errors > 0 { 1 } else { 0 }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        // ── format ─────────────────────────────────────────────────────────
        Command::Format { sql, dialect, file } => {
            match resolve_sql(sql, file) {
                Err(e) => { eprintln!("{e}"); 1 }
                Ok(src) => match parse(&src, &dialect) {
                    Err(e) => { eprintln!("{} {e}", "Parse error:".red().bold()); 1 }
                    Ok(stmts) => {
                        for stmt in &stmts { println!("{stmt}"); }
                        0
                    }
                }
            }
        }

        // ── ast ────────────────────────────────────────────────────────────
        Command::Ast { sql, dialect, file, output } => {
            match resolve_sql(sql, file) {
                Err(e) => { eprintln!("{e}"); 1 }
                Ok(src) => match parse(&src, &dialect) {
                    Err(e) => { eprintln!("{} {e}", "Parse error:".red().bold()); 1 }
                    Ok(stmts) => match render_ast(&stmts, &output) {
                        Ok(s) => { println!("{s}"); 0 }
                        Err(e) => { eprintln!("{e}"); 1 }
                    }
                }
            }
        }

        // ── validate ───────────────────────────────────────────────────────
        Command::Validate { sql, dialect, file, quiet } => {
            match resolve_sql(sql, file) {
                Err(e) => { eprintln!("{e}"); 1 }
                Ok(src) => validate_batch(&src, &dialect, quiet),
            }
        }

        // ── diff ───────────────────────────────────────────────────────────
        Command::Diff { sql1, sql2, dialect, file1, file2 } => {
            let s1 = resolve_sql(sql1, file1);
            let s2 = resolve_sql(sql2, file2);
            match (s1, s2) {
                (Err(e), _) | (_, Err(e)) => { eprintln!("{e}"); 1 }
                (Ok(a), Ok(b)) => diff_sql(&a, &b, &dialect),
            }
        }

        // ── repl ───────────────────────────────────────────────────────────
        Command::Repl { dialect } => {
            run_repl(&dialect);
            0
        }
    };

    std::process::exit(exit_code);
}