# FlowScope VS Code Extension

**FlowScope** is a powerful SQL lineage analysis and visualization tool for VS Code. It parses your SQL queries locally (powered by Rust/WASM) to extract column-level data lineage, identify table dependencies, and calculate query complexity metrics.

## Features

### 📊 Interactive Lineage Graph
Visualize the flow of data through your SQL queries.
- View relationships between tables, views, and CTEs.
- Click on nodes to highlight connections.
- Toggle between Table-level and Column-level lineage.
- **Command:** `FlowScope: Show Lineage Graph`

### 🔍 Rich Hover Information
Hover over any table or column in your SQL file to see:
- **Source/Origin:** Where the data comes from.
- **Filters:** `WHERE` and `JOIN` conditions affecting the table.
- **Complexity Score:** An algorithmic assessment of the query part's complexity.

### ⚙️ Multi-Dialect Support
Supports parsing for a wide range of SQL dialects including:
- PostgreSQL, MySQL, SQLite
- Snowflake, BigQuery, Redshift
- Databricks, DuckDB, ClickHouse
- T-SQL (MSSQL), Hive, ANSI SQL

## Usage

1. Open a `.sql` file in VS Code.
2. Run the command **`FlowScope: Show Lineage Graph`** (or click the icon in the editor title bar if available).
3. A side panel will open rendering the lineage graph for the active file.
4. Hover over parts of your SQL code to see immediate lineage context.

## Configuration

You can configure the extension via VS Code Settings (`Ctrl+,`):

| Setting | Default | Description |
|---------|---------|-------------|
| `flowscope.dialect` | `generic` | The SQL dialect used for parsing. Set this to your specific database (e.g., `snowflake`, `postgres`) for best results. |
| `flowscope.enableHover` | `true` | Enable/Disable the rich hover information tooltip. |
| `flowscope.enableCodeLens`| `true` | Enable/Disable complexity metrics appearing above SQL statements. |

## Development

This extension is part of the FlowScope monorepo. The webview UI participates in the root Yarn workspace, while the VS Code extension host is managed separately to ensure isolation.

### Prerequisites
- Node.js (v18+)
- Rust (for building the core WASM module)
- Yarn (for root workspace management)

### Building Locally

1. **Install Root Dependencies:**
   Navigate to the root of the repository and install dependencies (including those for the webview):
   ```bash
   yarn install
   ```

2. **Build Core WASM:**
   Ensure the core WASM package is built (referenced by the extension):
   ```bash
   npm run build:wasm
   ```

3. **Install Extension Dependencies:**
   Navigate to the `vscode` directory and install its specific dependencies:
   ```bash
   cd vscode
   npm install
   ```

4. **Build Extension & Webview:**
   From the `vscode` directory:
   ```bash
   npm run build
   ```
   This command builds both the extension host code (using `esbuild`) and the React webview (using `vite`).

5. **Run in Debug Mode:**
   - Open the project in VS Code.
   - Press `F5` to launch the **Extension Development Host**.

## License

Apache-2.0