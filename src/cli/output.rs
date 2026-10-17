use anyhow::Result;

pub fn json(value: &impl serde::Serialize) -> Result<()> {
    // stdout is reserved for JSON; diagnostics go to stderr.
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}
pub fn cell(value: &serde_json::Value) -> String {
    let text = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}
pub fn print_status_rows(rows: &[serde_json::Value]) {
    println!("PROJECT\tLABEL\tROLE\tSTATE\tVCS\tURL\tSOURCE");
    for row in rows {
        let workspace = &row["workspace"];
        let url = row["recorded_session"]["public_url"].as_str().map_or_else(
            || format!("https://{}", workspace["hostname"].as_str().unwrap_or("?")),
            str::to_owned,
        );
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            cell(&workspace["project_id"]),
            cell(&workspace["label"]),
            cell(&workspace["role"]),
            cell(&row["state"]),
            cell(&workspace["vcs"]),
            cell(&serde_json::Value::String(url)),
            cell(&workspace["source"])
        );
        if let Some(diagnostic) = row["diagnostic"].as_str() {
            eprintln!("parcela: {}: {diagnostic}", cell(&workspace["label"]));
        }
    }
}
