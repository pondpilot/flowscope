# Librarian — Manual Test Cases

Manual test cases for the Librarian feature. Automated tests live in `__tests__/`.

## Prerequisites

- FlowScope app running (`yarn dev` from `app/`)
- Valid AI provider configured (OpenAI, Anthropic, or custom endpoint)
- Sample PDFs available:
  - `SAP_Invoice_Approval_Technical_Documentation.pdf`
  - `SAP_Payment_Block_Reference.pdf`
  - `large_test_file.pdf` (>10 MB, for size limit test)
  - `test_document.docx` (for file type test)

### Test SQL script

Test cases assume the following SQL is loaded in the editor (SAP Accounts Payable: Invoice Processing Pipeline, standard SAP FI + MM tables):

```sql
-- SAP Accounts Payable: Invoice Processing Pipeline
-- Standard SAP tables only (FI + MM modules)

-- Invoice header with vendor and company details
SELECT
    rbkp.BELNR AS invoice_doc_number,
    rbkp.GJAHR AS fiscal_year,
    rbkp.BUKRS AS company_code,
    t001.BUTXT AS company_name,
    rbkp.LIFNR AS vendor_number,
    lfa1.NAME1 AS vendor_name,
    lfa1.LAND1 AS vendor_country,
    rbkp.RMWWR AS invoice_amount,
    rbkp.WAERS AS currency,
    rbkp.ZTERM AS payment_terms,
    rbkp.BLDAT AS invoice_date,
    rbkp.BUDAT AS posting_date,
    rbkp.RBSTAT AS invoice_status,
    rbkp.ZLSPR AS payment_block,
    rbkp.WMWST1 AS tax_amount
FROM RBKP AS rbkp
JOIN T001 AS t001
    ON rbkp.MANDT = t001.MANDT
    AND rbkp.BUKRS = t001.BUKRS
JOIN LFA1 AS lfa1
    ON rbkp.MANDT = lfa1.MANDT
    AND rbkp.LIFNR = lfa1.LIFNR;

-- Invoice line items linked to purchase orders
SELECT
    rseg.BELNR AS invoice_doc_number,
    rseg.GJAHR AS fiscal_year,
    rseg.BUZEI AS line_item,
    rseg.EBELN AS po_number,
    rseg.EBELP AS po_item,
    rseg.MATNR AS material_number,
    rseg.WRBTR AS item_amount,
    rseg.MENGE AS quantity,
    rseg.BSTME AS unit,
    rseg.KOSTL AS cost_center,
    ekko.BSART AS po_doc_type,
    ekko.BEDAT AS po_date,
    ekko.LIFNR AS po_vendor,
    ekpo.TXZ01 AS item_description,
    ekpo.NETPR AS po_net_price
FROM RSEG AS rseg
JOIN EKKO AS ekko
    ON rseg.MANDT = ekko.MANDT
    AND rseg.EBELN = ekko.EBELN
JOIN EKPO AS ekpo
    ON rseg.MANDT = ekpo.MANDT
    AND rseg.EBELN = ekpo.EBELN
    AND rseg.EBELP = ekpo.EBELP;

-- Accounting document entries for posted invoices
SELECT
    bkpf.BUKRS AS company_code,
    bkpf.BELNR AS accounting_doc_number,
    bkpf.GJAHR AS fiscal_year,
    bkpf.BLDAT AS document_date,
    bkpf.CPUDT AS entry_date,
    bkpf.USNAM AS posted_by,
    bkpf.BSTAT AS document_status,
    bseg.BUZEI AS line_item,
    bseg.KOART AS account_type,
    bseg.KONTO AS account_number,
    bseg.WRBTR AS amount,
    bseg.MWSTS AS tax_amount,
    bseg.KOSTL AS cost_center,
    bseg.AUGBL AS clearing_document,
    bseg.AUGDT AS clearing_date,
    bseg.ZLSPR AS payment_block
FROM BKPF AS bkpf
JOIN BSEG AS bseg
    ON bkpf.MANDT = bseg.MANDT
    AND bkpf.BUKRS = bseg.BUKRS
    AND bkpf.BELNR = bseg.BELNR
    AND bkpf.GJAHR = bseg.GJAHR;
```

---

## 1. Functional

| # | Test Case | Steps | Expected Result |
|---|-----------|-------|-----------------|
| 1 | Upload valid PDF via Upload panel | Click the drop zone and select `SAP_Invoice_Approval_Technical_Documentation.pdf` | File appears in the list with "ready" status |
| 2 | Upload valid PDF via drag-and-drop | Drag `SAP_Invoice_Approval_Technical_Documentation.pdf` onto the Librarian drop zone | File appears in the list (not intercepted by GlobalDropZone) |
| 3 | Upload PDF larger than 10 MB | Try to upload `large_test_file.pdf` | Error: "File exceeds 10 MB limit." |
| 4 | Upload non-PDF file | Try to upload `test_document.docx` | Error: "Only PDF files are supported." |
| 5 | Upload duplicate PDF | Upload `SAP_Invoice_Approval_Technical_Documentation.pdf`, then upload it again | Error: "A file with this name is already uploaded." |
| 6 | Upload multiple different PDFs | Upload `SAP_Payment_Block_Reference.pdf` + `SAP_Invoice_Approval_Technical_Documentation.pdf` | All files appear in the list |
| 7 | Delete PDF and verify | 1) Upload `SAP_Invoice_Approval_Technical_Documentation.pdf`; 2) Delete it; 3) Ask "Which SAP table contains details about invoice approvals?" | PDF removed from list; Documentation section in the answer has no PDF content |
| 8 | Chat scroll | Send 10+ messages | Auto-scrolls to the latest message; scrolling up works |
| 9 | Open/close panel | Toggle panel via the Librarian button (or ⌘L) | Panel opens and closes correctly |
| 10 | AI settings persistence | Configure AI, reload page | API key and model preserved (localStorage) |

---

## 2. Embedding Model

| # | Test Case | Steps | Expected Result |
|---|-----------|-------|-----------------|
| 11 | First load of embedding model | Clear browser cache, upload first PDF | Model downloads in background, UI remains responsive (not frozen) |
| 12 | Subsequent PDF upload | Upload second PDF after the first is processed | Model reused from cache, faster processing |

---

## 3. LLM Quality — Data Lineage (from SQL)

| # | Test Case | Question | Expected Answer |
|---|-----------|----------|-----------------|
| 13 | Off-topic question | "What is the capital of France?" | "I can only answer questions related to your data." |
| 14 | Chat context | 1) "What is BKPF?" → 2) "And what key fields does it have?" | Second answer refers to BKPF key fields without needing the full question |
| 15 | Table link | "How are BKPF and BSEG linked?" | Join on MANDT + BUKRS + BELNR + GJAHR (4-field key) |
| 16 | Invoice amount field | "What field stores the invoice amount?" | RBKP.RMWWR (aliased as `invoice_amount`) |
| 17 | Accounting document created date | "What column stores accounting document created date?" | BKPF.CPUDT (Entry Date) |
| 18 | Purchase order to invoice link | "What fields link purchase orders to invoices?" | RSEG.EBELN (PO number) + RSEG.EBELP (PO item), joined to EKKO and EKPO |
| 19 | Vendor country | "What column contains vendor country?" | LFA1.LAND1 (country) |
| 20 | Payment block | "Where is payment block stored?" | RBKP.ZLSPR (invoice level) and BSEG.ZLSPR (accounting level). Answer should appear in both Data Lineage and Documentation sections. |
| 21 | Tax amount | "Where is tax amount stored?" | RBKP.WMWST1 (header) and BSEG.MWSTS (line item) |
| 22 | RBKP vs RSEG | "Difference between RBKP and RSEG?" | RBKP = invoice header (1 record), RSEG = line items (many records) |

---

## 4. LLM Quality — Documentation (from PDF)

Requires `SAP_Invoice_Approval_Technical_Documentation.pdf` uploaded.

| # | Test Case | Question | Expected Answer |
|---|-----------|----------|-----------------|
| 23 | Approval statuses | "Possible statuses in approval table?" | ZSTATUS: 01=Pending, 02=Approved, 03=Rejected |
| 24 | Approval table key | "Key for ZSAP_INV_APPROVAL?" | MANDT + ZINV_ID |
| 25 | Rejection reason storage | "Where is rejection reason stored?" | ZSAP_INV_APPROVAL.ZCOMMENT (CHAR 255) |

---

## Notes

- **Response format**: Every on-topic answer should include three sections: **Summary**, **Data Lineage**, **Documentation**. Each section either contains relevant info or "No information." (exactly).
- **Inline code formatting**: Table and column names should render with colored styling (accent color) in the chat.
- **Source attribution**: Documentation answers should cite the source PDF file name.
- **Off-topic refusal**: Only the refusal sentence — no Summary / Data Lineage / Documentation sections.
