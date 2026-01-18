# flowscope-export

Database export helpers for FlowScope analysis results.

## Overview

`flowscope-export` provides utilities to persist `flowscope-core` lineage results to a database for downstream inspection or visualization. The default backend is DuckDB, which can be disabled by turning off the `duckdb` feature.

## Features

- `duckdb` (default): Export lineage to a DuckDB database file.

## Usage

Add it to your project alongside `flowscope-core`:

```toml
[dependencies]
flowscope-core = "0.1.0"
flowscope-export = "0.1.0"
```

## License

Apache 2.0
