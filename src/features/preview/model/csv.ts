// CSV / TSV 分隔符的共享常量与类型。解析本身已移到后端（core/csv_table.rs，经
// fs_csv_preview 在 Rust 侧把镜像内文件解析成结构化表格）——前端只在工具栏选择分隔符、
// 给后端传分隔符提示时用到这里的字面量。CsvViewer 渲染后端返回的表格并负责搜索/排序。

/** 支持的分隔符候选。`\t` = TSV，其余为 CSV 变体（含欧式分号、竖线）。 */
export const DELIMITERS = [',', ';', '\t', '|'] as const;
export type Delimiter = (typeof DELIMITERS)[number];
