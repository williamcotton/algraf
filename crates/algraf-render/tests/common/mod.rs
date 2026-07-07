#![allow(dead_code)]

use std::collections::HashMap;

use algraf_data::{read_csv_str, ColumnDef, DataFrame, Table};
use algraf_render::{
    render, render_draw_list, DrawList, ImageAsset, ImageAssets, RenderOptions, RenderResult, Theme,
};
use algraf_semantics::{analyze, analyze_with_tables, ChartIr};
use algraf_syntax::parse;

pub struct RenderFixture<'a> {
    source: &'a str,
    primary_csv: &'a str,
    tables: &'a [(&'a str, &'a str)],
}

impl<'a> RenderFixture<'a> {
    pub fn new(source: &'a str, primary_csv: &'a str) -> Self {
        Self {
            source,
            primary_csv,
            tables: &[],
        }
    }

    pub fn with_tables(
        source: &'a str,
        primary_csv: &'a str,
        tables: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            source,
            primary_csv,
            tables,
        }
    }

    pub fn render_result(&self) -> RenderResult {
        let frame = read_frame(self.primary_csv, "csv");
        let (named, schemas) = named_table_fixtures(self.tables);
        let ir = analyze_fixture(self.source, &frame, &schemas);
        render(
            &ir,
            &frame,
            &Theme::minimal(),
            render_options_for_named_tables(&named),
        )
        .expect("render")
    }

    pub fn draw_list(&self) -> DrawList {
        let frame = read_frame(self.primary_csv, "csv");
        let (named, schemas) = named_table_fixtures(self.tables);
        let ir = analyze_fixture(self.source, &frame, &schemas);
        render_draw_list(
            &ir,
            &frame,
            &Theme::minimal(),
            render_options_for_named_tables(&named),
        )
        .expect("draw list")
        .draw_list
    }

    pub fn svg(&self) -> String {
        self.render_result().svg
    }
}

pub fn render_svg(source: &str, csv: &str) -> String {
    RenderFixture::new(source, csv).svg()
}

pub fn render_result(source: &str, csv: &str) -> RenderResult {
    RenderFixture::new(source, csv).render_result()
}

pub fn render_result_with_tables(
    source: &str,
    primary_csv: &str,
    tables: &[(&str, &str)],
) -> RenderResult {
    RenderFixture::with_tables(source, primary_csv, tables).render_result()
}

pub fn draw_list(source: &str, csv: &str) -> DrawList {
    RenderFixture::new(source, csv).draw_list()
}

pub fn draw_list_with_tables(source: &str, primary_csv: &str, tables: &[(&str, &str)]) -> DrawList {
    RenderFixture::with_tables(source, primary_csv, tables).draw_list()
}

pub fn svg(source: &str, csv: &str) -> String {
    render_svg(source, csv)
}

pub fn svg_with_tables(source: &str, primary_csv: &str, tables: &[(&str, &str)]) -> String {
    render_result_with_tables(source, primary_csv, tables).svg
}

pub fn image_assets() -> ImageAssets {
    let mut assets = ImageAssets::new();
    assets.insert(ImageAsset {
        source: "a.png".to_string(),
        href: "data:image/png;base64,AAAA".to_string(),
        intrinsic_width: 2.0,
        intrinsic_height: 1.0,
    });
    assets.insert(ImageAsset {
        source: "b.png".to_string(),
        href: "data:image/png;base64,BBBB".to_string(),
        intrinsic_width: 1.0,
        intrinsic_height: 2.0,
    });
    assets
}

pub fn render_result_with_assets(source: &str, csv: &str, assets: &ImageAssets) -> RenderResult {
    let frame = read_frame(csv, "csv");
    let ir = analyze_fixture(source, &frame, &HashMap::new());
    render(
        &ir,
        &frame,
        &Theme::minimal(),
        RenderOptions::default().with_image_assets(assets),
    )
    .expect("render")
}

pub fn draw_list_with_assets(source: &str, csv: &str, assets: &ImageAssets) -> DrawList {
    let frame = read_frame(csv, "csv");
    let ir = analyze_fixture(source, &frame, &HashMap::new());
    render_draw_list(
        &ir,
        &frame,
        &Theme::minimal(),
        RenderOptions::default().with_image_assets(assets),
    )
    .expect("draw list")
    .draw_list
}

fn read_frame(csv: &str, label: &str) -> DataFrame {
    read_csv_str(csv)
        .unwrap_or_else(|err| panic!("{label}: {err}"))
        .frame
}

fn named_table_fixtures(
    tables: &[(&str, &str)],
) -> (HashMap<String, DataFrame>, HashMap<String, Vec<ColumnDef>>) {
    let mut named = HashMap::<String, DataFrame>::new();
    let mut schemas = HashMap::new();
    for (name, csv) in tables {
        let table = read_frame(csv, "named csv");
        schemas.insert((*name).to_string(), table.schema().to_vec());
        named.insert((*name).to_string(), table);
    }
    (named, schemas)
}

fn analyze_fixture(
    source: &str,
    frame: &DataFrame,
    schemas: &HashMap<String, Vec<ColumnDef>>,
) -> ChartIr {
    let parsed = parse(source);
    if schemas.is_empty() {
        return analyze(&parsed.syntax(), frame.schema()).ir.expect("ir");
    }

    let mut analysis = analyze_with_tables(&parsed.syntax(), frame.schema(), schemas);
    let mut diagnostics = parsed.into_diagnostics();
    diagnostics.append(&mut analysis.diagnostics);
    assert!(
        diagnostics.iter().all(|d| !d.code.starts_with('E')),
        "{diagnostics:#?}"
    );
    analysis.ir.expect("ir")
}

fn render_options_for_named_tables<'a>(named: &'a HashMap<String, DataFrame>) -> RenderOptions<'a> {
    if named.is_empty() {
        RenderOptions::default()
    } else {
        RenderOptions::default().with_named_tables(named)
    }
}
