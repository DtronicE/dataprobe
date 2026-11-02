# ⚡ Dataprobe

**Dataprobe** is an enterprise-grade, high-performance data auditing, cleaning, and validation engine written in Rust. Powered by Apache Arrow memory layouts (`polars`), BLAKE3 SIMD parallel hashing (`rayon`), and multi-format streaming I/O.

---

## 🚀 Performance Benchmarks

Tested on **100,000 records** (Windows x86_64 Release Build):

| Operation | Execution Time | Description |
| :--- | :--- | :--- |
| **Clean (CSV → Parquet)** | `~47 ms` | String whitespace trimming & binary columnar conversion |
| **Validate (Schema Rules)** | `~66 ms` | JSON rule assertion checks on 100k rows |
| **HashDedupe (BLAKE3 SIMD)**| `~150 ms` | Multi-column content hash deduplication across CPU threads |

---

## 🛠️ CLI Installation & Usage

Dataprobe is distributed as a standalone, zero-dependency executable (`.\dist\dataprobe.exe`).

### 1. Audit Dataset Schema & Null Counts
```powershell
.\dist\dataprobe.exe audit --input data.csv
