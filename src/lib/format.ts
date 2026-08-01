// 数字 / 时间展示工具。

/** token 数中文格式化：>=1亿 → X.X亿；>=1万 → X.X万；否则千分位。 */
export function formatTokens(n: number): string {
    if (n >= 1e8) return `${trim1(n / 1e8)}亿`;
    if (n >= 1e4) return `${trim1(n / 1e4)}万`;
    return n.toLocaleString();
}

function trim1(v: number): string {
    const s = v.toFixed(1);
    return s.endsWith(".0") ? s.slice(0, -2) : s;
}

/** 金额（元）保留两位。 */
export function formatYuan(v: number): string {
    return `¥${v.toFixed(2)}`;
}

/** YYYY-MM-DD（本地时区），offsetDays 相对今天偏移。 */
export function localDate(offsetDays = 0): string {
    const d = new Date();
    d.setDate(d.getDate() + offsetDays);
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, "0");
    const day = String(d.getDate()).padStart(2, "0");
    return `${y}-${m}-${day}`;
}

/** 柱状图日期标签：今天/昨天/周几简写，返回 MM-DD。 */
export function shortDate(date: string): string {
    return date.slice(5);
}

/** 时间戳/ISO 字符串 → 本地 "YYYY-MM-DD HH:mm:ss"。 */
export function formatDateTime(input: number | string | null | undefined): string {
    if (input == null) return "";
    const d = typeof input === "number" ? new Date(input) : new Date(input);
    if (Number.isNaN(d.getTime())) return String(input);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
