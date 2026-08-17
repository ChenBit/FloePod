/** 展示格式化工具 */

export function formatSize(bytes: number): string {
  if (bytes <= 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  const n = v >= 100 || i === 0 ? Math.round(v).toString() : v.toFixed(1);
  return `${n} ${units[i]}`;
}

const two = (n: number) => n.toString().padStart(2, "0");

export function formatTime(unixMs: number): string {
  const d = new Date(unixMs);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  const hm = `${two(d.getHours())}:${two(d.getMinutes())}`;
  if (sameDay) return hm;
  const sameYear = d.getFullYear() === now.getFullYear();
  const md = `${d.getMonth() + 1}月${d.getDate()}日`;
  return sameYear ? `${md} ${hm}` : `${d.getFullYear()}年${md}`;
}

export function kindLabel(kind: string): string {
  switch (kind) {
    case "folder":
      return "文件夹";
    case "text":
      return "文字";
    case "shortcut":
      return "快捷方式";
    default:
      return "文件";
  }
}
