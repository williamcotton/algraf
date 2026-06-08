import React from "react";

import {
  CITY_MIX_CSV,
  DISTRIBUTION_CSV,
  FORECAST_CSV,
  HEIGHTS_CSV,
  INSET_CITIES_CSV,
  PANELS_CSV,
  PENGUINS_CSV,
  PROFIT_CSV,
  REGIONS_CSV,
  SALES_CSV,
  SCORES_CSV,
} from "./datasets";

export interface DocExample {
  id: string;
  source: string;
  files: Record<string, string>;
  assets?: Record<string, string>;
}

export interface DocSection {
  id: string;
  title: string;
  body: React.ReactNode;
  example?: DocExample;
}

export interface DocTopic {
  slug: string; // "" is the overview at /docs
  nav: string;
  title: string;
  lede: React.ReactNode;
  sections: DocSection[];
}

const PENGUIN_FILES = { "penguins.csv": PENGUINS_CSV };
const SALES_FILES = { "sales.csv": SALES_CSV };
const PROFIT_FILES = { "profit.csv": PROFIT_CSV };
const FORECAST_FILES = { "forecast.csv": FORECAST_CSV };
const REGION_FILES = { "regions.csv": REGIONS_CSV };
const PANEL_FILES = { "panels.csv": PANELS_CSV };
const HEIGHT_FILES = { "heights.csv": HEIGHTS_CSV };
const DISTRIBUTION_FILES = { "distribution.csv": DISTRIBUTION_CSV };
const SCORES_FILES = { "scores.csv": SCORES_CSV };
const INSET_PIE_FILES = { "inset_cities.csv": INSET_CITIES_CSV, "city_population_mix.csv": CITY_MIX_CSV };
const INSET_PIE_ASSETS = { "us_counties.geojson": "data/us_counties.geojson" };

// `Code` keeps inline tokens visually distinct from prose.
function Code({ children }: { children: React.ReactNode }): React.ReactElement {
  return <code className="doc-inline-code">{children}</code>;
}

export const DOC_TOPICS: DocTopic[] = [
  {
    slug: "",
    nav: "Overview",
    title: "Algraf documentation",
    lede: (
      <>
        Algraf is a block-scoped, algebraic grammar-of-graphics language. You describe a chart, and the
        same toolchain validates it against your data, trains scales, and renders deterministic SVG. These
        pages explain the language one topic at a time — every example is a live editor you can edit and
        re-render in place.
      </>
    ),
    sections: [
      {
        id: "model",
        title: "The mental model",
        body: (
          <>
            <p>
              Every chart is three nested ideas. <Code>Chart(...)</Code> names the data source and canvas.
              Inside it, a <Code>Space(...)</Code> turns columns into a coordinate system using the{" "}
              <em>algebra</em>. Inside the space, <strong>geometries</strong> like <Code>Point</Code>,{" "}
              <Code>Line</Code>, and <Code>Bar</Code> draw one mark per row.
            </p>
            <p>
              Aesthetics such as <Code>fill: species</Code> are <em>mappings</em>, not literal colors:
              Algraf trains a scale from the column, colors the marks, and emits a matching legend. A literal
              like <Code>fill: "#4E79A7"</Code> is used verbatim instead.
            </p>
          </>
        ),
        example: {
          id: "overview-first-chart",
          files: PENGUIN_FILES,
          source: `Chart(data: "penguins.csv", width: 640, height: 380, title: "Penguin body mass") {
    Theme(name: "minimal")
    Scale(fill: species, palette: "accent")
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length_mm * body_mass_g) {
        Point(fill: species, size: 5, alpha: 0.85)
    }
}
`,
        },
      },
      {
        id: "where-next",
        title: "Where to go next",
        body: (
          <>
            <p>
              The <strong>Algebra</strong> page is the core idea: the operators <Code>*</Code>,{" "}
              <Code>/</Code>, and <Code>+</Code> are how you build scatter plots, dodged bars, facets, and
              ribbons from the same three rules. From there, the <strong>Bars</strong>,{" "}
              <strong>Facets</strong>, <strong>Glyphs</strong>, and <strong>Statistics</strong> pages each
              focus on one family of charts. <strong>Theming &amp; guides</strong> covers presentation, and{" "}
              <strong>Tooling</strong> shows the CLI, editor, and browser runtime.
            </p>
          </>
        ),
      },
    ],
  },

  {
    slug: "algebra",
    nav: "The algebra",
    title: "The algebra",
    lede: (
      <>
        The space algebra is what makes Algraf more than a chart template. Three operators —{" "}
        <Code>*</Code> (cross), <Code>/</Code> (nest), and <Code>+</Code> (blend) — combine columns into a
        coordinate system. They are <em>structural</em>, not arithmetic: <Code>a * b</Code> does not
        multiply values, it pairs their domains. The same handful of rules produce scatter plots, grouped
        bars, facets, and ribbons.
      </>
    ),
    sections: [
      {
        id: "cross",
        title: "Cross: a * b",
        body: (
          <>
            <p>
              Cross forms a Cartesian product of two domains. The left operand is the physical{" "}
              <strong>x</strong> axis and the right operand is the physical <strong>y</strong> axis, so{" "}
              <Code>flipper_length_mm * body_mass_g</Code> is an ordinary scatter frame. Swapping the order
              swaps the axes — that is also how you get horizontal bars later.
            </p>
          </>
        ),
        example: {
          id: "algebra-cross",
          files: PENGUIN_FILES,
          source: `Chart(data: "penguins.csv", width: 620, height: 380, title: "Cross: x * y") {
    Theme(name: "minimal")

    Space(flipper_length_mm * body_mass_g) {
        Point(fill: species, size: 5, alpha: 0.85)
    }
}
`,
        },
      },
      {
        id: "nest",
        title: "Nest: a / b",
        body: (
          <>
            <p>
              Nest conditions one domain inside another. In a position context, <Code>quarter / type</Code>{" "}
              allocates a band per <Code>quarter</Code>, then a sub-band per <Code>type</Code> inside it.
              That is exactly a dodged / grouped bar chart, with no special &ldquo;grouped bar&rdquo; mode —
              it falls out of the algebra. The full frame <Code>(quarter / type) * amount</Code> then crosses
              those banded slots with the measured value.
            </p>
          </>
        ),
        example: {
          id: "algebra-nest",
          files: SALES_FILES,
          source: `Chart(data: "sales.csv", width: 660, height: 400, title: "Nest: dodged bars from (quarter / type)") {
    Theme(name: "minimal")
    Scale(fill: type, palette: "accent")

    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
`,
        },
      },
      {
        id: "blend",
        title: "Blend: (a + b)",
        body: (
          <>
            <p>
              Blend unions two domains into one shared dimension. In a continuous context the union spans the
              minimum and maximum of both columns, which is what a band geometry needs.{" "}
              <Code>day * (lower + upper)</Code> trains a single y scale across both bounds so a{" "}
              <Code>Ribbon</Code> can close the area between them. Blend must be parenthesized — bare{" "}
              <Code>lower + upper</Code> is rejected so the structure stays explicit.
            </p>
          </>
        ),
        example: {
          id: "algebra-blend",
          files: FORECAST_FILES,
          source: `Chart(data: "forecast.csv", width: 640, height: 380, marginRight: 24, title: "Blend: day * (lower + upper)") {
    Theme(name: "minimal")

    Space(day * (lower + upper)) {
        Ribbon(ymin: lower, ymax: upper, fill: "#4E79A7", alpha: 0.3)
    }
}
`,
        },
      },
      {
        id: "facet-frame",
        title: "Nesting a whole plane: facets",
        body: (
          <>
            <p>
              When the left side of a nest is itself a Cartesian plane, the result is a facet layout:{" "}
              <Code>(time * sales) / region</Code> draws one panel per region, each sharing the trained x and
              y scales by default. Faceting is the same nest operator applied one level up — see the{" "}
              <strong>Facets</strong> page for grids and per-panel scales.
            </p>
          </>
        ),
        example: {
          id: "algebra-facet",
          files: REGION_FILES,
          source: `Chart(data: "regions.csv", width: 700, height: 380, title: "Facet: (time * sales) / region") {
    Theme(name: "minimal")

    Space((time * sales) / region) {
        Line(stroke: product, strokeWidth: 2)
    }
}
`,
        },
      },
      {
        id: "precedence",
        title: "Precedence and parentheses",
        body: (
          <>
            <p>
              Nest binds tighter than cross, which binds tighter than blend, and every operator is
              left-associative. So <Code>quarter / type * amount</Code> means{" "}
              <Code>(quarter / type) * amount</Code>. Parentheses make intent explicit and are{" "}
              <em>required</em> for blend. A 3-D Cartesian frame like <Code>x * y * z</Code> is rejected; when{" "}
              <Code>z</Code> is categorical the analyzer suggests <Code>(x * y) / z</Code> instead.
            </p>
          </>
        ),
      },
    ],
  },

  {
    slug: "bars",
    nav: "Bars & layouts",
    title: "Bars & layouts",
    lede: (
      <>
        Bar charts show off how much the algebra and a single <Code>layout</Code> property can do. Grouping
        comes from the nest operator; stacking, proportional fill, and diverging baselines come from the bar{" "}
        <Code>layout</Code>; orientation comes from the order of the frame.
      </>
    ),
    sections: [
      {
        id: "grouped",
        title: "Grouped / dodged bars",
        body: (
          <p>
            Nest the group column inside the category band — <Code>(quarter / type) * amount</Code> — and
            map <Code>fill: type</Code>. Each quarter becomes a cluster of side-by-side bars with no extra
            configuration.
          </p>
        ),
        example: {
          id: "bars-grouped",
          files: SALES_FILES,
          source: `Chart(data: "sales.csv", width: 660, height: 400, title: "Grouped bars") {
    Theme(name: "minimal")
    Scale(fill: type, palette: "accent")

    Space((quarter / type) * amount) {
        Bar(fill: type)
    }
}
`,
        },
      },
      {
        id: "stacked",
        title: "Stacked bars",
        body: (
          <p>
            Keep a single <Code>quarter</Code> band and switch the bar <Code>layout</Code> to{" "}
            <Code>"stack"</Code>. The type contributions stack within each quarter instead of sitting
            side-by-side.
          </p>
        ),
        example: {
          id: "bars-stacked",
          files: SALES_FILES,
          source: `Chart(data: "sales.csv", width: 660, height: 400, title: "Stacked bars") {
    Theme(name: "minimal")
    Scale(fill: type, palette: "accent")

    Space(quarter * amount) {
        Bar(fill: type, layout: "stack")
    }
}
`,
        },
      },
      {
        id: "fill",
        title: "Proportional fill bars",
        body: (
          <p>
            <Code>layout: "fill"</Code> normalizes every stack to 1.0, so the bars compare shares instead of
            totals — useful for part-to-whole composition over a category.
          </p>
        ),
        example: {
          id: "bars-fill",
          files: SALES_FILES,
          source: `Chart(data: "sales.csv", width: 660, height: 400, title: "Proportional fill bars") {
    Theme(name: "minimal")
    Scale(fill: type, palette: "accent")

    Space(quarter * amount) {
        Bar(fill: type, layout: "fill")
    }
}
`,
        },
      },
      {
        id: "diverging",
        title: "Diverging bars around a baseline",
        body: (
          <p>
            Bars extend in both directions from zero when the value column is signed. A manual color scale
            highlights positive versus negative, and an <Code>HLine</Code> draws the zero baseline. Tick
            labels rotate with <Code>tickLabelAngle</Code>.
          </p>
        ),
        example: {
          id: "bars-diverging",
          files: PROFIT_FILES,
          source: `Chart(data: "profit.csv", width: 680, height: 400, title: "Monthly profit / loss") {
    Theme(name: "minimal")
    Scale(fill: status, range: ["Profit" => "#2ca02c", "Loss" => "#d62728"])
    Guide(axis: x, label: "Month", tickLabelAngle: -45)
    Guide(axis: y, label: "Profit / Loss ($)")

    Space(month * profit) {
        Bar(fill: status, layout: "stack", alpha: 0.85)
        HLine(y: 0, stroke: "#333333", strokeWidth: 1.2)
    }
}
`,
        },
      },
      {
        id: "horizontal",
        title: "Horizontal bars",
        body: (
          <p>
            The left side of <Code>*</Code> is always physical x and the right side is physical y. Put the
            value on x and the (nested) category on y for horizontal grouped bars — the orientation is just a
            consequence of the frame order, not a separate flag.
          </p>
        ),
        example: {
          id: "bars-horizontal",
          files: SALES_FILES,
          source: `Chart(data: "sales.csv", width: 660, height: 420, title: "Horizontal grouped bars") {
    Theme(name: "minimal")
    Scale(fill: type, palette: "accent")
    Guide(axis: x, label: "Amount")
    Guide(axis: y, label: "Quarter")

    Space(amount * (quarter / type)) {
        Bar(fill: type, alpha: 0.9)
    }
}
`,
        },
      },
    ],
  },

  {
    slug: "facets",
    nav: "Facets",
    title: "Facets",
    lede: (
      <>
        Faceting splits one chart into a grid of small multiples that share scales, so panels stay
        comparable. It is the nest operator applied to a whole plane, with <Code>Layout(...)</Code> options
        for column counts, explicit row/column grids, and per-panel scales.
      </>
    ),
    sections: [
      {
        id: "wrap",
        title: "Facet wrap",
        body: (
          <p>
            <Code>(x * y) / group</Code> wraps one panel per category into rows and columns automatically.
            All panels share x and y by default so differences in level are honest. Override the column count
            with <Code>Layout(facetColumns: n)</Code>.
          </p>
        ),
        example: {
          id: "facets-wrap",
          files: REGION_FILES,
          source: `Chart(data: "regions.csv", width: 700, height: 400, title: "One panel per region") {
    Theme(name: "minimal")

    Space((time * sales) / region) {
        Line(stroke: product, strokeWidth: 2)
    }
}
`,
        },
      },
      {
        id: "grid",
        title: "Facet grids",
        body: (
          <p>
            For a true two-way grid, name a row column and a column column with{" "}
            <Code>Layout(facetRows: ..., facetCols: ...)</Code>. Panels lay out row-major over the cross
            product of the two domains. <Code>facetLabel</Code> controls the strip text and{" "}
            <Code>panelSpacing</Code> the gaps.
          </p>
        ),
        example: {
          id: "facets-grid",
          files: PANEL_FILES,
          source: `Chart(data: "panels.csv", width: 680, height: 460, title: "Facet grid by band") {
    Theme(name: "minimal")
    Layout(facetRows: row_band, facetCols: col_band,
           facetLabel: "name-value", panelSpacing: [24, 48])

    Space(x * y) {
        Point(fill: series, size: 6)
    }
}
`,
        },
      },
      {
        id: "free",
        title: "Free panel scales",
        body: (
          <p>
            Shared scales are the default, but when panels live on wildly different ranges you can train each
            panel locally with <Code>Layout(facetScales: "free")</Code> (or <Code>"free-x"</Code> /{" "}
            <Code>"free-y"</Code> to free only one axis). Legends stay chart-level and are not retrained per
            panel.
          </p>
        ),
        example: {
          id: "facets-free",
          files: PANEL_FILES,
          source: `Chart(data: "panels.csv", width: 700, height: 320, title: "Free facet scales") {
    Theme(name: "minimal")
    Layout(facetCols: series, facetScales: "free")

    Space(x * y) {
        Line(stroke: series, strokeWidth: 2)
        Point(fill: series, size: 5)
    }
}
`,
        },
      },
    ],
  },

  {
    slug: "glyphs",
    nav: "Glyphs",
    title: "Glyphs",
    lede: (
      <>
        A <Code>Glyph</Code> declares a chart-valued mark: a reusable, chart-scoped template that renders
        once per matched host row. Each host row keeps its position in the outer space while a matched child
        table draws a miniature chart — a pie, sparkline, or composite glyph — right at that point. They
        are at their best as map marks.
      </>
    ),
    sections: [
      {
        id: "pies",
        title: "Pie glyphs on a projected map",
        body: (
          <>
            <p>
              Here a county basemap (<Code>Geo</Code> over a projected <Code>geom</Code> space) sits under a
              second space of city anchors. At each city, the <Code>pie</Code> glyph draws a population-mix
              chart: the child rows come from <Code>city_population_mix.csv</Code> correlated by{" "}
              <Code>key: [city]</Code>, and the glyph&apos;s diameter is mapped from the host city&apos;s
              <Code>population</Code> through the ordinary <Code>Scale(size:, range:)</Code>.
            </p>
            <p>
              The pie itself is just a polar child space: <Code>Space(count, coords: "polar", theta: "y")</Code>{" "}
              with a fill-layout <Code>Bar</Code>. Because the glyph is a real chart, the same projection and
              basemap anchor every instance in place. (First render fetches a 1.3&nbsp;MB counties basemap, so
              it takes a moment.)
            </p>
          </>
        ),
        example: {
          id: "glyphs-city-pies",
          files: INSET_PIE_FILES,
          assets: INSET_PIE_ASSETS,
          source: `Chart(data: GeoJson("us_counties.geojson"), width: 820, height: 520,
       title: "Major cities with population mix pies",
       subtitle: "Glyph pie charts projected onto the county basemap") {
    Theme(name: "void")
    Table cities = "inset_cities.csv"
    Table city_mix = "city_population_mix.csv"

    Glyph pie(data: city_mix, key: [city], scales: "shared") {
        Space(count, coords: "polar", theta: "y") {
            Scale(fill: age_group,
                  range: ["under 25" => "#4E79A7", "25-64" => "#F28E2B", "65+" => "#59A14F"],
                  label: "Age group")
            Bar(fill: age_group, layout: "fill")
        }
    }

    Scale(size: population, range: [20, 48], label: "Population")

    Space(geom, projection: "albers_usa") {
        Geo(fill: "#f3f4f6", stroke: "#d1d5db", strokeWidth: 0.25)
    }

    Space(long * lat, projection: "albers_usa", data: cities) {
        pie(size: population, clip: "circle", padding: 1)
        Text(label: city, dy: -25, size: 7, fill: "#1f2937", anchor: "middle")
    }
}
`,
        },
      },
      {
        id: "nesting",
        title: "How the join works",
        body: (
          <>
            <p>
              Two ideas make glyphs composable. First, <Code>key</Code> correlates the child table to the
              host row: each child column resolves up the host row-context chain by name, and an
              <Code>outer.&lt;column&gt;</Code> qualifier forces the nearest enclosing glyph host when names
              shadow. Second, glyph marks can <strong>nest</strong> — a glyph&apos;s child space can invoke
              another glyph — so you can build composite marks such as a category pie with a trend sparkline
              inside each slice. Start simple and add depth only where it earns its keep.
            </p>
          </>
        ),
      },
    ],
  },

  {
    slug: "stats",
    nav: "Statistics",
    title: "Statistical geometries",
    lede: (
      <>
        Some geometries summarize data before drawing: they bin, fit, or reduce the rows and render the
        result. They share the same space algebra, so a histogram or a smooth lives in the same coordinate
        system as the raw marks beside it.
      </>
    ),
    sections: [
      {
        id: "histogram",
        title: "Histograms",
        body: (
          <p>
            A one-dimensional space <Code>Space(value)</Code> plus <Code>Histogram(...)</Code> bins the
            column and draws the counts. Control the bins with <Code>bins</Code>, <Code>binWidth</Code>, and{" "}
            <Code>boundary</Code>. (Mapping <Code>fill</Code> to a category stacks the groups within each
            bin.)
          </p>
        ),
        example: {
          id: "stats-histogram",
          files: DISTRIBUTION_FILES,
          source: `Chart(data: "distribution.csv", width: 660, height: 400, title: "Distribution") {
    Theme(name: "minimal")

    Space(value) {
        Histogram(bins: 16, fill: "#4E79A7", stroke: "#ffffff", strokeWidth: 1, alpha: 0.9)
    }
}
`,
        },
      },
      {
        id: "dodged-histogram",
        title: "Dodged histograms (the nest operator again)",
        body: (
          <p>
            To place groups side-by-side instead of stacked, nest the group inside the binned value axis with{" "}
            <Code>score / cohort</Code> — the same algebraic move that dodges bars. Each bin splits into one
            sub-bar per cohort on a continuous axis.
          </p>
        ),
        example: {
          id: "stats-dodged-histogram",
          files: SCORES_FILES,
          source: `Chart(data: "scores.csv", width: 680, height: 400, title: "Exam scores by cohort (dodged)") {
    Theme(name: "minimal")
    Guide(axis: x, label: "Score")
    Guide(axis: y, label: "Count")

    Space(score / cohort) {
        Histogram(fill: cohort, binWidth: 5, boundary: 45)
    }
}
`,
        },
      },
      {
        id: "smooth",
        title: "Trend lines with Smooth",
        body: (
          <p>
            <Code>Smooth(method: "lm")</Code> fits a linear model and draws the line in the same x/y space as
            the points it sits beside. <Code>method: "loess"</Code> fits a local curve instead, and{" "}
            <Code>se: true</Code> adds a confidence band.
          </p>
        ),
        example: {
          id: "stats-smooth",
          files: PENGUIN_FILES,
          source: `Chart(data: "penguins.csv", width: 660, height: 400, title: "Linear fit") {
    Theme(name: "minimal")

    Space(flipper_length_mm * body_mass_g) {
        Point(fill: species, alpha: 0.6, size: 4)
        Smooth(method: "lm", stroke: "#263238", strokeWidth: 2)
    }
}
`,
        },
      },
      {
        id: "distributions",
        title: "Distributions: violin + boxplot",
        body: (
          <p>
            Geometries compose inside one space. A translucent <Code>Violin</Code> shows the density and a
            narrow opaque <Code>Boxplot</Code> overlays the summary statistics, both per category band.
          </p>
        ),
        example: {
          id: "stats-violin",
          files: HEIGHT_FILES,
          source: `Chart(data: "heights.csv", width: 660, height: 420, title: "Height by group") {
    Theme(name: "minimal")

    Space(gender * height) {
        Violin(fill: gender, alpha: 0.45)
        Boxplot(width: 15, fill: "#ffffff", stroke: "#2b2b2b", strokeWidth: 1.5)
    }
}
`,
        },
      },
    ],
  },

  {
    slug: "theming",
    nav: "Theming & guides",
    title: "Theming & guides",
    lede: (
      <>
        Presentation is separate from the data mapping. <Code>Theme</Code> picks an overall look,{" "}
        <Code>Scale</Code> tunes how columns map to color and size, and <Code>Guide</Code> controls axes and
        labels. None of them change which rows are drawn — only how they read.
      </>
    ),
    sections: [
      {
        id: "themes",
        title: "Themes and scales",
        body: (
          <>
            <p>
              <Code>Theme(name: ...)</Code> selects a preset — try <Code>"minimal"</Code>,{" "}
              <Code>"classic"</Code>, or <Code>"void"</Code>. <Code>Scale(fill: species, palette: "accent")</Code>{" "}
              chooses the categorical palette and emits the legend, while <Code>Guide(axis: ..., label: ...)</Code>{" "}
              names each axis. Edit the theme name or palette below and re-render to compare.
            </p>
          </>
        ),
        example: {
          id: "theming-themes",
          files: PENGUIN_FILES,
          source: `Chart(data: "penguins.csv", width: 660, height: 400,
       title: "Penguin body mass", subtitle: "Palmer Station") {
    Theme(name: "minimal")
    Scale(fill: species, palette: "accent")
    Guide(axis: x, label: "Flipper length (mm)")
    Guide(axis: y, label: "Body mass (g)")

    Space(flipper_length_mm * body_mass_g) {
        Point(fill: species, size: 6, alpha: 0.85)
    }
}
`,
        },
      },
      {
        id: "manual",
        title: "Manual colors and domains",
        body: (
          <p>
            A <Code>range</Code> with <Code>"key" =&gt; "color"</Code> pairs assigns exact colors to known
            categories, and <Code>Scale(axis: ..., domain: [...])</Code> pins an axis range. Continuous fills
            accept a <Code>gradient</Code> of color stops. These keep output stable for reports where the
            mapping must not drift.
          </p>
        ),
        example: {
          id: "theming-manual",
          files: PROFIT_FILES,
          source: `Chart(data: "profit.csv", width: 680, height: 400, title: "Fixed status colors") {
    Theme(name: "classic")
    Scale(fill: status, range: ["Profit" => "#2ca02c", "Loss" => "#d62728"])
    Scale(axis: y, domain: [-20000, 45000])
    Guide(axis: x, label: "Month")
    Guide(axis: y, label: "Profit / Loss ($)")

    Space(month * profit) {
        Bar(fill: status, layout: "stack", alpha: 0.9)
        HLine(y: 0, stroke: "#333333", strokeWidth: 1.2)
    }
}
`,
        },
      },
    ],
  },

  {
    slug: "tooling",
    nav: "Tooling",
    title: "Tooling",
    lede: (
      <>
        Algraf ships parser, analyzer, language server, renderer, and browser runtime as one binary, so the
        same engine backs the CLI, your editor, and the previews on this site.
      </>
    ),
    sections: [
      {
        id: "install",
        title: "Install",
        body: (
          <>
            <p>Install the packaged binary with Homebrew, then call `algraf` directly.</p>
            <pre className="doc-codeblock">
              <code>{`brew tap williamcotton/algraf
brew install algraf`}</code>
            </pre>
          </>
        ),
      },
      {
        id: "cli",
        title: "Command line",
        body: (
          <>
            <p>
              <Code>check</Code> parses and analyzes without rendering, <Code>render</Code> emits SVG or PNG,
              and <Code>schema</Code> prints the inferred column types. <Code>format</Code>, <Code>ast</Code>
              , and <Code>ir</Code> round out the inspection commands.
            </p>
            <pre className="doc-codeblock">
              <code>{`algraf check chart.ag
algraf render chart.ag --output chart.svg
algraf render chart.ag --output chart.png
algraf schema chart.ag --json`}</code>
            </pre>
          </>
        ),
      },
      {
        id: "editor",
        title: "Editor & language server",
        body: (
          <p>
            <Code>algraf lsp</Code> is a language server with diagnostics, hover, completion, semantic
            tokens, formatting, and code actions. The VS Code extension is a thin client that spawns it; the
            live editors on this site call the same analysis compiled to WebAssembly.
          </p>
        ),
      },
      {
        id: "browser",
        title: "Browser runtime",
        body: (
          <>
            <p>
              The <Code>algraf-wasm</Code> crate exposes the driver and renderer to JavaScript with an
              in-memory file system, which is what powers every preview here. To run this site locally:
            </p>
            <pre className="doc-codeblock">
              <code>{`cd demo
npm install
npm run dev`}</code>
            </pre>
          </>
        ),
      },
    ],
  },
];

export function topicForSlug(slug: string): DocTopic {
  return DOC_TOPICS.find((topic) => topic.slug === slug) ?? DOC_TOPICS[0];
}
