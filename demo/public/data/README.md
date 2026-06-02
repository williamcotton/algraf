# Demo Data

The browser demo checks these public datasets into `demo/public/data/` so the
WASM site is self-contained after install.

| File | Rows | Source |
| --- | ---: | --- |
| `penguins.csv` | 344 | Palmer Penguins CSV from `allisonhorst/palmerpenguins`: <https://raw.githubusercontent.com/allisonhorst/palmerpenguins/main/inst/extdata/penguins.csv> |
| `gapminder.csv` | 1,704 | Plotly sample Gapminder CSV: <https://raw.githubusercontent.com/plotly/datasets/master/gapminderDataFiveYear.csv> |
| `iris.csv` | 150 | Plotly sample Iris CSV: <https://raw.githubusercontent.com/plotly/datasets/master/iris-data.csv> |
| `stocks.csv` | 559 | Vega Datasets stock prices CSV: <https://raw.githubusercontent.com/vega/vega-datasets/main/data/stocks.csv> |
| `seattle-weather.csv` | 1,461 | Vega Datasets Seattle weather CSV: <https://raw.githubusercontent.com/vega/vega-datasets/main/data/seattle-weather.csv> |
| `astronauts.csv` | 564 | Algraf example fixture from `examples/astronauts.csv`. |
| `minard_troops.csv` | 50 | Algraf example fixture from `examples/minard_troops.csv`. |
| `minard_cities.csv` | 19 | Algraf example fixture from `examples/minard_cities.csv`. |
| `homepage-starter.csv` | 12 | Small checked-in fixture for `demo/public/homepage.ag`. |

`penguins.json` is the original tiny playground fixture retained for
compatibility with older local builds.
