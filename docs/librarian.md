# Librarian — User Guide

Librarian is an AI-powered chat panel inside FlowScope that answers questions about your SQL based on data lineage analysis and uploaded PDF documentation. It runs entirely in the browser — no backend.

## Opening the Panel

- Click the **Librarian** button in the analysis toolbar (next to **Schema**)
- Or press **⌘L** / **Ctrl+L**

The panel opens on the right side of the workspace. Its open/closed state is remembered across reloads.

## Configuring an AI Provider

Before sending your first question, configure an AI provider:

1. Click the **⚙ Settings** icon in the Librarian panel header
2. Select a provider:
   - **OpenAI** — uses `https://api.openai.com/v1/chat/completions`
   - **Anthropic** — uses `https://api.anthropic.com/v1/messages`
   - **Custom** — any OpenAI-compatible endpoint (e.g. LiteLLM, Azure OpenAI, local Ollama via proxy)
3. Paste your **API key**
4. Enter the **model** name (e.g. `gpt-4o`, `claude-sonnet-4-6`, or your custom model ID)
5. For **Custom**, also enter the **endpoint URL**
6. Optionally edit the **System Prompt**. The default prompt is prefilled and can be restored with **Reset to default**.
7. Click **Save**

Your API key is stored in the browser's `localStorage` and is only ever sent to the provider you configured. It never reaches FlowScope's servers.

## Asking Questions

Paste SQL into the editor, wait for lineage analysis to render, then type your question in the Librarian chat:

> "Where is payment block stored?"
>
> "How are BKPF and BSEG linked?"
>
> "What field stores the invoice amount?"

The answer has three sections:

- **Summary** — a concrete 1–2 sentence answer with specific table and column names
- **Data Lineage** — information derived from your SQL analysis (joins, column mappings, transformations)
- **Documentation** — information from uploaded PDFs, citing the source file name

If the question is unrelated to your data, Librarian replies:

> _"I can only answer questions related to your data."_

Identifiers like `BKPF`, `MANDT` are visually highlighted in the response so they're easy to scan.

### Jump to Lineage from a chat answer

Click an assistant message to switch to the **Lineage** tab and highlight what the answer is about. Navigation reads only the **Summary** section of the answer (not Data Lineage or Documentation), so the click reflects the main claim and ignores tables mentioned only as supporting context.

What happens on click:

- The first referenced **column** (or, if the Summary mentions only a table, the first table) is written to the lineage search box. Every matching table card lights up its border, and matching columns inside the cards get the highlight color.
- "Show column edges" auto-enables when a column is referenced, so individual column rows are visually distinct inside the table cards.
- The viewport gently pans and pulses on the source table that contains the column. Real source tables (`type === 'table'`) are preferred over views and CTEs that only reference the column transitively.
- Identifier matching is case-insensitive — `bkpf.MANDT`, `BKPF.mandt`, and `BKPF.MANDT` all resolve the same way.

If the answer references identifiers that aren't present in the current lineage (e.g. a column from a PDF only), those references are skipped. A click on a Summary with no resolvable identifier is a no-op — the active tab does not change.

> **Tip:** the lineage search box accepts a single substring, not a regex. Heterogeneous references in one Summary (e.g. both `MANDT` and `BUKRS`) cannot share a single search term — the first column wins. Clear the search box manually to dismiss the highlight.

## Uploading PDF Documentation

Librarian can answer questions using technical documentation you upload (e.g. table specs, business logic reference, ERD docs).

1. Expand the **Documentation** section at the bottom of the panel
2. Either click the drop zone or drag a PDF onto it
3. Wait for processing — text extraction + embedding generation runs in a Web Worker
4. The file appears with a ✓ when ready

**Limits:**

- Max **10 MB** per file
- Only `.pdf` files
- Unlimited file count (browser memory permitting)
- Duplicate file names are rejected

**How it works:** each page's text is extracted, split into ~500-character chunks, and embedded using a local model (`multilingual-e5-small`, ~118 MB, cached in the browser after first load). When you ask a question, the top 5 most relevant chunks are included in the prompt.

The embedding model is multilingual — German, Russian, and other non-English documentation works out of the box.

To remove a document, click the trash icon next to its name. Both the file entry and its embedded chunks are cleared.

## Chat History

- The full chat history is shown in the panel
- Only the **last 10 messages** are sent to the AI as context (to keep prompts small)
- After each request, the panel shows the last prompt's raw character and byte size above the chat input
- History is **not** persisted across reloads — a page refresh starts a fresh conversation
- Chat and uploaded PDFs are **scoped to the active project** — switching projects shows that project's own conversation and documents, and questions only see the active project's PDFs and chat history. State stays in memory only, so a reload clears every project's Librarian state.

## Keyboard Shortcuts

| Shortcut        | Action                 |
| --------------- | ---------------------- |
| `⌘L` / `Ctrl+L` | Toggle Librarian panel |
| `Enter`         | Send message           |
| `Shift+Enter`   | Newline in input       |

## Help Icon

The `?` icon in the panel header opens a quick popover with a brief description and usage tips.

## Troubleshooting

**"Please configure your AI settings first."**
Open Settings (⚙) and fill in provider, API key, and model.

**Drag-and-drop PDF doesn't land in Librarian.**
Make sure you drop the file onto the Librarian drop zone. If the file-drop overlay appears over the whole window and seems to catch the drop, try using the click-to-upload button instead.

**First PDF upload takes a long time.**
The embedding model (~118 MB) is downloaded on first use and cached. Subsequent PDFs process much faster.

**"Based on the current data, there is no information on your question."**
Librarian found no relevant context in your SQL, lineage, or PDFs to answer. Try pasting more SQL, uploading more documentation, or rephrasing the question.

**API errors** (401, 403, 429, 5xx).
Check that the API key is valid and has quota. For Anthropic, ensure the model name matches an available Claude model. For Custom endpoints, verify the URL is reachable and OpenAI-compatible.

## Privacy

- All processing (PDF extraction, embeddings, vector search) runs locally in your browser
- Your SQL, PDFs, and questions are sent **only** to the AI provider you configured
- No telemetry, no analytics, no third-party tracking

## What Librarian Does NOT Do

- No code execution — Librarian cannot run SQL or connect to databases
- No streaming responses — answers arrive once complete
- No persistent chat history — closed tabs lose their conversations
- No image extraction from PDFs — text only
