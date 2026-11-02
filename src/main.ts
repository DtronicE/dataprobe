import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

interface ExtensionCount {
  extension: string;
  count: number;
}

interface DatasetInfo {
  file_name: string;
  file_path: string;
  format: string;
  row_count: number;
  column_count: number;
}

interface CleanOptions {
  remove_duplicates: boolean;
  drop_null_rows: boolean;
  drop_null_cols: boolean;
  trim_whitespace: boolean;
  fill_nulls: boolean;
  fill_value: string;
}

interface CleanResult {
  original_rows: number;
  cleaned_rows: number;
  original_cols: number;
  cleaned_cols: number;
  duplicates_removed: number;
  memory_reduced_mb: number;
  completeness_pct: number;
  output_path: string;
}

interface BatchResult {
  files_processed: number;
  total_memory_reduced_mb: number;
  total_rows_cleaned: number;
}

interface PreviewData {
  original_rows: number;
  original_cols: number;
  cleaned_rows: number;
  cleaned_cols: number;
  original_headers: string[];
  cleaned_headers: string[];
  original_preview: string[][];
  cleaned_preview: string[][];
}

interface ScanResult {
  path: string;
  status: string;
  files_scanned: number;
  total_bytes: number;
  execution_time_ms: number;
  top_extensions: ExtensionCount[];
  datasets: DatasetInfo[];
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 Bytes";
  const k = 1024;
  const sizes = ["Bytes", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

let scanInputEl: HTMLInputElement | null;
let statusMsgEl: HTMLElement | null;
let statFilesEl: HTMLElement | null;
let statSizeEl: HTMLElement | null;
let statTimeEl: HTMLElement | null;
let extSectionEl: HTMLElement | null;
let extTableBodyEl: HTMLElement | null;
let datasetSectionEl: HTMLElement | null;
let datasetTableBodyEl: HTMLElement | null;
let metricsSectionEl: HTMLElement | null;
let metricMemEl: HTMLElement | null;
let metricCompleteEl: HTMLElement | null;
let metricRowsEl: HTMLElement | null;

let modalEl: HTMLElement | null;
let prevOrigHeadEl: HTMLElement | null;
let prevOrigBodyEl: HTMLElement | null;
let prevCleanHeadEl: HTMLElement | null;
let prevCleanBodyEl: HTMLElement | null;
let prevOrigDimEl: HTMLElement | null;
let prevCleanDimEl: HTMLElement | null;

let activeFilePath: string | null = null;
let activeFileName: string | null = null;
let activeFormat: string | null = null;
let currentScanResult: ScanResult | null = null;

function getOptions(): CleanOptions {
  return {
    remove_duplicates: (document.querySelector("#opt-dedupe") as HTMLInputElement).checked,
    drop_null_rows: (document.querySelector("#opt-drop-null-rows") as HTMLInputElement).checked,
    drop_null_cols: (document.querySelector("#opt-drop-null-cols") as HTMLInputElement).checked,
    trim_whitespace: (document.querySelector("#opt-trim") as HTMLInputElement).checked,
    fill_nulls: (document.querySelector("#opt-fill-nulls") as HTMLInputElement).checked,
    fill_value: (document.querySelector("#opt-fill-val") as HTMLInputElement).value || "0",
  };
}

async function selectFolder() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select Directory to Scan",
    });

    if (selected && scanInputEl) {
      scanInputEl.value = selected as string;
    }
  } catch (error) {
    console.error("Folder picker error:", error);
  }
}

function renderTableGrid(headEl: HTMLElement, bodyEl: HTMLElement, headers: string[], grid: string[][]) {
  headEl.innerHTML = `<tr>${headers.map((h) => `<th>${h}</th>`).join("")}</tr>`;
  bodyEl.innerHTML = grid
    .map((row) => `<tr>${row.map((val) => `<td>${val}</td>`).join("")}</tr>`)
    .join("");
}

async function openPreview(filePath: string, fileName: string, format: string) {
  activeFilePath = filePath;
  activeFileName = fileName;
  activeFormat = format;

  if (statusMsgEl) statusMsgEl.textContent = `Generating preview for ${fileName}...`;

  try {
    const options = getOptions();
    const data = await invoke<PreviewData>("preview_clean_dataset", { filePath, options });

    if (prevOrigDimEl) prevOrigDimEl.textContent = `${data.original_rows} rows x ${data.original_cols} cols`;
    if (prevCleanDimEl) prevCleanDimEl.textContent = `${data.cleaned_rows} rows x ${data.cleaned_cols} cols`;

    if (prevOrigHeadEl && prevOrigBodyEl) renderTableGrid(prevOrigHeadEl, prevOrigBodyEl, data.original_headers, data.original_preview);
    if (prevCleanHeadEl && prevCleanBodyEl) renderTableGrid(prevCleanHeadEl, prevCleanBodyEl, data.cleaned_headers, data.cleaned_preview);

    if (modalEl) modalEl.classList.remove("hidden");
    if (statusMsgEl) statusMsgEl.textContent = `Preview loaded for ${fileName}`;
  } catch (error) {
    console.error("Preview error:", error);
    if (statusMsgEl) statusMsgEl.textContent = `Preview Error: ${error}`;
  }
}

async function confirmAndSave() {
  if (!activeFilePath || !activeFileName || !activeFormat) return;

  try {
    const output_path = await save({
      defaultPath: `cleaned_${activeFileName}`,
      filters: [{ name: `${activeFormat.toUpperCase()} File`, extensions: [activeFormat] }],
    });

    if (!output_path) return;

    if (modalEl) modalEl.classList.add("hidden");
    if (statusMsgEl) statusMsgEl.textContent = `Processing and saving cleaned dataset...`;

    const options = getOptions();
    const result = await invoke<CleanResult>("clean_and_save_dataset", {
      filePath: activeFilePath,
      outputPath: output_path,
      options,
    });

    if (statusMsgEl) {
      statusMsgEl.textContent = `Cleaned dataset saved: ${result.cleaned_rows} rows saved to ${result.output_path}`;
    }

    if (metricMemEl) metricMemEl.textContent = `${result.memory_reduced_mb.toFixed(2)} MB`;
    if (metricCompleteEl) metricCompleteEl.textContent = `${result.completeness_pct.toFixed(1)}%`;
    if (metricRowsEl) metricRowsEl.textContent = `${result.cleaned_rows.toLocaleString()} / ${result.original_rows.toLocaleString()}`;
    if (metricsSectionEl) metricsSectionEl.classList.remove("hidden");
  } catch (error) {
    console.error("Save error:", error);
    if (statusMsgEl) statusMsgEl.textContent = `Save Error: ${error}`;
  }
}

async function cleanAllDatasets() {
  if (!currentScanResult || currentScanResult.datasets.length === 0) return;

  try {
    const outputDir = await open({
      directory: true,
      multiple: false,
      title: "Select Directory to Save Cleaned Datasets",
    });

    if (!outputDir) return;

    const targetFormat = (document.querySelector("#batch-fmt-select") as HTMLSelectElement).value;
    const filePaths = currentScanResult.datasets.map((ds) => ds.file_path);
    const options = getOptions();

    if (statusMsgEl) statusMsgEl.textContent = `Batch cleaning ${filePaths.length} datasets...`;

    const res = await invoke<BatchResult>("clean_batch_datasets", {
      filePaths,
      outputDir: outputDir as string,
      targetFormat,
      options,
    });

    if (statusMsgEl) {
      statusMsgEl.textContent = `Batch complete: ${res.files_processed} files processed, ${res.total_rows_cleaned.toLocaleString()} rows saved in ${outputDir}`;
    }

    if (metricMemEl) metricMemEl.textContent = `${res.total_memory_reduced_mb.toFixed(2)} MB`;
    if (metricCompleteEl) metricCompleteEl.textContent = `100%`;
    if (metricRowsEl) metricRowsEl.textContent = `${res.total_rows_cleaned.toLocaleString()} total`;
    if (metricsSectionEl) metricsSectionEl.classList.remove("hidden");
  } catch (error) {
    console.error("Batch cleaning error:", error);
    if (statusMsgEl) statusMsgEl.textContent = `Batch Error: ${error}`;
  }
}

async function exportCSV() {
  if (!currentScanResult) return;

  try {
    const savePath = await save({
      defaultPath: "dataprobe_scan_report.csv",
      filters: [{ name: "CSV File", extensions: ["csv"] }],
    });

    if (savePath) {
      let csvContent = `Scanned Path,${currentScanResult.path}\n`;
      csvContent += `Files Scanned,${currentScanResult.files_scanned}\n`;
      csvContent += `Total Bytes,${currentScanResult.total_bytes}\n`;
      csvContent += `Execution Time (ms),${currentScanResult.execution_time_ms}\n\n`;
      csvContent += `Extension,Count\n`;

      currentScanResult.top_extensions.forEach((item) => {
        csvContent += `.${item.extension},${item.count}\n`;
      });

      await invoke("export_scan_csv", { savePath, content: csvContent });
      if (statusMsgEl) statusMsgEl.textContent = `Report saved to: ${savePath}`;
    }
  } catch (error) {
    console.error("Export error:", error);
  }
}

async function scan() {
  if (!scanInputEl || !statusMsgEl || !statFilesEl || !statSizeEl || !statTimeEl) return;

  const path = scanInputEl.value.trim();
  if (!path) {
    statusMsgEl.textContent = "Please select or enter a valid directory path.";
    return;
  }

  statusMsgEl.textContent = `Scanning: ${path}...`;
  statFilesEl.textContent = "-";
  statSizeEl.textContent = "-";
  statTimeEl.textContent = "-";
  if (extSectionEl) extSectionEl.classList.add("hidden");
  if (datasetSectionEl) datasetSectionEl.classList.add("hidden");
  if (metricsSectionEl) metricsSectionEl.classList.add("hidden");

  try {
    const result = await invoke<ScanResult>("run_dataprobe_scan", { path });
    currentScanResult = result;

    statusMsgEl.textContent = `Scan complete for: ${result.path}`;
    statFilesEl.textContent = result.files_scanned.toLocaleString();
    statSizeEl.textContent = formatBytes(result.total_bytes);
    statTimeEl.textContent = `${result.execution_time_ms} ms`;

    if (extTableBodyEl && extSectionEl && result.top_extensions.length > 0) {
      const targetTableBody = extTableBodyEl;
      targetTableBody.innerHTML = "";
      result.top_extensions.forEach((item) => {
        const row = document.createElement("tr");
        row.innerHTML = `<td>.${item.extension}</td><td>${item.count.toLocaleString()}</td>`;
        targetTableBody.appendChild(row);
      });
      extSectionEl.classList.remove("hidden");
    }

    if (datasetTableBodyEl && datasetSectionEl && result.datasets.length > 0) {
      const targetDatasetBody = datasetTableBodyEl;
      targetDatasetBody.innerHTML = "";
      result.datasets.forEach((ds) => {
        const row = document.createElement("tr");
        row.innerHTML = `
          <td>${ds.file_name}</td>
          <td>.${ds.format}</td>
          <td>${ds.row_count.toLocaleString()}</td>
          <td>${ds.column_count}</td>
          <td><button class="clean-btn" data-path="${ds.file_path}" data-name="${ds.file_name}" data-format="${ds.format}">Clean Dataset</button></td>
        `;
        targetDatasetBody.appendChild(row);
      });

      document.querySelectorAll(".clean-btn").forEach((btn) => {
        btn.addEventListener("click", (e) => {
          const target = e.target as HTMLElement;
          const filePath = target.getAttribute("data-path");
          const fileName = target.getAttribute("data-name");
          const format = target.getAttribute("data-format");
          if (filePath && fileName && format) openPreview(filePath, fileName, format);
        });
      });

      datasetSectionEl.classList.remove("hidden");
    }
  } catch (error) {
    console.error("IPC Scan Error:", error);
    statusMsgEl.textContent = `Error: ${error}`;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  scanInputEl = document.querySelector("#scan-input");
  statusMsgEl = document.querySelector("#status-message");
  statFilesEl = document.querySelector("#stat-files");
  statSizeEl = document.querySelector("#stat-size");
  statTimeEl = document.querySelector("#stat-time");
  extSectionEl = document.querySelector("#ext-section");
  extTableBodyEl = document.querySelector("#ext-table-body");
  datasetSectionEl = document.querySelector("#dataset-section");
  datasetTableBodyEl = document.querySelector("#dataset-table-body");
  metricsSectionEl = document.querySelector("#metrics-section");
  metricMemEl = document.querySelector("#metric-mem");
  metricCompleteEl = document.querySelector("#metric-complete");
  metricRowsEl = document.querySelector("#metric-rows");

  modalEl = document.querySelector("#preview-modal");
  prevOrigHeadEl = document.querySelector("#prev-orig-head");
  prevOrigBodyEl = document.querySelector("#prev-orig-body");
  prevCleanHeadEl = document.querySelector("#prev-clean-head");
  prevCleanBodyEl = document.querySelector("#prev-clean-body");
  prevOrigDimEl = document.querySelector("#prev-orig-dim");
  prevCleanDimEl = document.querySelector("#prev-clean-dim");

  document.querySelector("#browse-btn")?.addEventListener("click", selectFolder);
  document.querySelector("#export-btn")?.addEventListener("click", exportCSV);
  document.querySelector("#batch-clean-btn")?.addEventListener("click", cleanAllDatasets);
  document.querySelector("#confirm-save-btn")?.addEventListener("click", confirmAndSave);
  document.querySelector("#modal-close-btn")?.addEventListener("click", () => modalEl?.classList.add("hidden"));
  document.querySelector("#scan-form")?.addEventListener("submit", (e) => {
    e.preventDefault();
    scan();
  });
});
