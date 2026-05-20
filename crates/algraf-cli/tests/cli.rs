use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_algraf")
}

fn temp_dir(test: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("algraf-cli-{test}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let data = dir.join("data.csv");
    let chart = dir.join("chart.ag");
    fs::write(&data, "x,y,group\n1,2,a\n3,4,b\n").unwrap();
    fs::write(
        &chart,
        "Chart(data: \"data.csv\") {\n  Space(x * y) {\n    Point(fill: group)\n  }\n}\n",
    )
    .unwrap();
    (chart, data)
}

#[test]
fn render_writes_svg_to_stdout() {
    let dir = temp_dir("render");
    let (chart, _) = write_fixture(&dir);

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("<svg"));
}

#[test]
fn render_debug_flags_add_deterministic_svg_content() {
    let dir = temp_dir("render-debug");
    let (chart, _) = write_fixture(&dir);

    let output = Command::new(bin())
        .arg("render")
        .arg(&chart)
        .arg("--emit-metadata")
        .arg("--debug-layout")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("<!-- algraf metadata:"));
    assert!(out.contains("class=\"algraf-debug-layout\""));
}

#[test]
fn schema_reads_source_from_stdin_once() {
    let dir = temp_dir("schema-stdin");
    let _ = write_fixture(&dir);
    let source = "Chart(data: \"data.csv\") {\n  Space(x * y) { Point() }\n}\n";

    let mut child = Command::new(bin())
        .arg("schema")
        .arg("-")
        .arg("--json")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("\"name\": \"x\""), "stdout: {out}");
}

#[test]
fn missing_chart_data_is_a_diagnostic() {
    let dir = temp_dir("missing-data");
    let chart = dir.join("chart.ag");
    fs::write(&chart, "Chart() {}\n").unwrap();

    let output = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("E1001"));
}

#[test]
fn data_flag_does_not_replace_missing_chart_data() {
    let dir = temp_dir("data-override-missing");
    let chart = dir.join("chart.ag");
    let data = dir.join("data.csv");
    fs::write(&chart, "Chart() {}\n").unwrap();
    fs::write(&data, "x,y\n1,2\n").unwrap();

    let output = Command::new(bin())
        .arg("check")
        .arg(&chart)
        .arg("--data")
        .arg(&data)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("E1001"));
}

#[test]
fn source_and_csv_cannot_both_read_from_stdin() {
    let source = "Chart(data: \"ignored.csv\") {\n  Space(x * y) { Point() }\n}\n";
    let mut child = Command::new(bin())
        .arg("render")
        .arg("-")
        .arg("--data")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("cannot read both source and CSV data from stdin"));
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
