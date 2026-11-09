export function truncate(str, maxLen) {
    if (str.length <= maxLen)
        return str;
    return str.slice(0, maxLen - 3) + "...";
}
export function extractTextFromParts(parts) {
    return parts
        .filter((p) => p.type === "text" && "text" in p && Boolean(p.text))
        .map((p) => p.text)
        .join(" ");
}
