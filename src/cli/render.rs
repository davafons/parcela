//! Human renderings of command results. JSON output never passes through here.
use super::output::{Style, cell, status_rows, table, url_of};
use serde_json::Value;

pub fn status(row: &Value, style: &Style) -> String {
    let workspace = &row["workspace"];
    let mut text = format!(
        "{} {}  {}\n  {}\n",
        style.bold(&cell(&workspace["project_id"])),
        cell(&workspace["label"]),
        style.state(&cell(&row["state"])),
        style.url(&url_of(row)),
    );
    let endpoints = row["live"]["endpoints"].as_object();
    if let Some(endpoints) = endpoints.filter(|endpoints| endpoints.len() > 1) {
        let rows: Vec<Vec<String>> = endpoints
            .iter()
            .map(|(service, endpoint)| {
                vec![
                    format!("  {service}"),
                    if endpoint["http_ready"] == true {
                        style.state("ready")
                    } else {
                        style.state("unhealthy")
                    },
                    cell(&endpoint["public_url"]),
                ]
            })
            .collect();
        text.push_str(&table(&["  SERVICE", "HTTP", "URL"], &rows));
    }
    text.push_str(&format!(
        "  {}\n",
        style.dim(&format!("path {}", cell(&workspace["source"])))
    ));
    text
}

pub fn inventory(rows: &[Value], style: &Style) -> String {
    status_rows(rows, style)
}

pub fn context(value: &Value, style: &Style) -> String {
    let workspace = &value["workspace"];
    let mut text = format!(
        "{} {}  {}\n",
        style.bold(&cell(&workspace["project_id"])),
        cell(&workspace["label"]),
        cell(&workspace["role"]),
    );
    if let Some(urls) = value["planned_urls"].as_object() {
        for (service, url) in urls {
            text.push_str(&format!("  {service:<10}{}\n", style.url(&cell(url))));
        }
    }
    text.push_str(&format!("  {:<10}{}\n", "path", cell(&workspace["source"])));
    text.push_str(&format!(
        "  {:<10}{}\n",
        "runtime",
        cell(&value["runtime_dir"])
    ));
    text.push_str(&format!("  {:<10}{}\n", "id", cell(&workspace["id"])));
    text
}

pub fn services(value: &Value, style: &Style) -> String {
    let rows: Vec<Vec<String>> = value["services"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|service| {
            vec![
                cell(&service["name"]),
                style.state(&cell(&service["status"]).to_ascii_lowercase()),
                cell(&service["pid"]),
            ]
        })
        .collect();
    table(&["SERVICE", "STATE", "PID"], &rows)
}

/// How people refer to an environment: the project for its primary checkout, else the preview name.
pub fn name(workspace: &Value) -> String {
    if workspace["role"] == "primary" {
        cell(&workspace["project_id"])
    } else {
        format!(
            "{} ({})",
            cell(&workspace["label"]),
            cell(&workspace["project_id"])
        )
    }
}

pub fn down(value: &Value, label: &str, style: &Style) -> String {
    if value["already_stopped"] == true {
        format!("{} {label} was already stopped\n", style.ok())
    } else {
        format!(
            "{} Stopped {label}; source and data preserved\n",
            style.ok()
        )
    }
}

pub fn started(value: &Value, style: &Style) -> String {
    let url = cell(&value["session"]["public_url"]);
    let mut text = format!(
        "{} {} is running at {}\n",
        style.ok(),
        name(&value["workspace"]),
        style.url(&url)
    );
    let others: Vec<&Value> = value["session"]["endpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|endpoint| endpoint["public_url"].as_str() != Some(url.as_str()))
        .collect();
    for endpoint in others {
        text.push_str(&format!(
            "  {:<10}{}\n",
            cell(&endpoint["service"]),
            style.url(&cell(&endpoint["public_url"]))
        ));
    }
    text.push_str(&format!(
        "\n  {}    show service output\n  {}  check health\n  {}    stop it\n",
        style.bold("parcela logs"),
        style.bold("parcela status"),
        style.bold("parcela down"),
    ));
    text
}

pub fn recovered(value: &Value, style: &Style) -> String {
    if value["state"] == "stopped" {
        format!("{} Finished the interrupted stop\n", style.ok())
    } else {
        format!(
            "{} Recovered the running services at {}\n",
            style.ok(),
            style.url(&cell(&value["session"]["public_url"]))
        )
    }
}

pub fn proxy(value: &Value, style: &Style) -> String {
    let router = &value["router"];
    format!(
        "{} Proxy running (HTTPS {}, HTTP {})\n",
        style.ok(),
        cell(&router["https_port"]),
        cell(&router["http_port"])
    )
}

pub fn discover(value: &Value, _style: &Style) -> String {
    let rows: Vec<Vec<String>> = value["checkouts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            vec![
                cell(&row["vcs"]),
                cell(&row["name"]),
                row["refs"]
                    .as_array()
                    .map(|refs| refs.iter().map(cell).collect::<Vec<_>>().join(","))
                    .unwrap_or_default(),
                cell(&row["adopted_environment_id"]),
                cell(&row["source"]),
            ]
        })
        .collect();
    table(&["VCS", "NAME", "REFS", "ENVIRONMENT", "PATH"], &rows)
}

pub fn config(value: &Value, style: &Style) -> String {
    format!(
        "{} {} is valid\n  {:<10}{}\n",
        style.ok(),
        cell(&value["manifest"]),
        "project",
        cell(&value["project_id"]),
    )
}

/// Fallback for results without a dedicated layout: one `key value` line per field.
pub fn fields(value: &Value, _style: &Style) -> String {
    let Some(object) = value.as_object() else {
        return format!("{}\n", cell(value));
    };
    let width = object.keys().map(String::len).max().unwrap_or(0) + 2;
    object
        .iter()
        .map(|(key, value)| format!("{key:<width$}{}\n", cell(value)))
        .collect()
}
