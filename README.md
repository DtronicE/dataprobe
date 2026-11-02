# 🚀 Dataprobe

Dataprobe is a high-performance desktop data cleaning & batch transformation suite built with Tauri v2, Rust, and Polars. Effortlessly inspect, trim, deduplicate, fill nulls, and convert CSV, Parquet, and JSON datasets with real-time preview and memory profiling.

---

## 📥 Download & Installation

Download standalone binaries directly from the official release page:

- 💻 **[Download Windows GUI Setup (.exe)](https://github.com/DtronicE/dataprobe/releases/download/v0.1.0/Dataprobe_0.1.0_x64-setup.exe)**
- 📦 **[Download Windows MSI Package (.msi)](https://github.com/DtronicE/dataprobe/releases/download/v0.1.0/Dataprobe_0.1.0_x64_en-US.msi)**
- ⚡ **[Download Standalone CLI Executable (.exe)](https://github.com/DtronicE/dataprobe/releases/download/v0.1.0/dataprobe-cli-x64.exe)**

---

## ⚙️ CLI Quick Start & Command Guide

Dataprobe can be run directly from terminal/PowerShell for automated batch scripts and headless workflows.

### Basic Command Syntax

```bash
# Basic Usage
dataprobe-cli-x64.exe --input <PATH> --output <PATH> [OPTIONS]

# Quick Dataset Inspection
dataprobe-cli-x64.exe inspect --input ./data/raw_dataset.csv

# Batch Transformation Example
dataprobe-cli-x64.exe transform \
  --input ./data/sales_raw.csv \
  --output ./data/sales_cleaned.parquet \
  --format parquet \
  --trim-strings \
  --drop-nulls \
  --dedupe
