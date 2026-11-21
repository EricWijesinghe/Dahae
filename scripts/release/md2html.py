# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Renders one Markdown file to a standalone HTML page (stdlib only, no network).

  python -I scripts/release/md2html.py <in.md> <out.html> "<page title>"
  python -I scripts/release/md2html.py --fragment <in.md> <out.html>   (body only, for the manual book)

Covers what the Malkkeum guides and the manual use: headings (with GitHub-style ids), paragraphs,
nested lists, tables, fenced code, blockquotes, images, bold, italics, inline code and links.
Anything else is escaped and shown as text.
"""
import html
import re
import sys
from pathlib import Path

CSS = """
body{font-family:"Malgun Gothic","Segoe UI",system-ui,sans-serif;max-width:860px;margin:2rem auto;
padding:0 1rem;line-height:1.6;color:#1d1d1f}
h1,h2,h3{line-height:1.25}h1{border-bottom:2px solid #7c5cff;padding-bottom:.3rem}
code,pre{font-family:Consolas,monospace;background:#f3f3f7;border-radius:4px}
code{padding:.1rem .3rem}pre{padding:.8rem;overflow-x:auto}pre code{padding:0}
table{border-collapse:collapse;margin:1rem 0;width:100%}
th,td{border:1px solid #d6d6de;padding:.4rem .6rem;text-align:left;vertical-align:top}
th{background:#f3f3f7}blockquote{border-left:4px solid #7c5cff;margin:1rem 0;padding:.2rem 1rem;color:#444}
a{color:#5b3fd6}img{max-width:100%}
@media (prefers-color-scheme: dark){body{background:#16161a;color:#eaeaf0}code,pre,th{background:#26262e}
th,td{border-color:#3a3a46}blockquote{color:#c8c8d0}a{color:#b7a6ff}}
"""

LIST_ITEM = re.compile(r"^(\s*)([-*]|\d+\.)\s+")
IMAGE = re.compile(r"^!\[([^\]]*)\]\(([^)\s]+)\)$")
BLOCK_START = re.compile(r"^(#|```|[-*] |\d+\. |\||>|!\[)")


def inline(text):
    # Code spans are swapped for placeholders so their contents stay literal and bold can wrap them.
    codes = []

    def stash(m):
        codes.append("<code>" + html.escape(m.group(1)) + "</code>")
        return f"\x00{len(codes) - 1}\x00"

    s = html.escape(re.sub(r"`([^`]+)`", stash, text), quote=False)
    s = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", lambda m: f'<a href="{m.group(2)}">{m.group(1)}</a>', s)
    s = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"(?<![*\w])\*([^*\s][^*]*?)\*(?![*\w])", r"<em>\1</em>", s)
    return re.sub(r"\x00(\d+)\x00", lambda m: codes[int(m.group(1))], s)


def table_cells(line):
    # `\|` is a literal pipe inside a cell.
    row = line.strip().removeprefix("|")
    if row.endswith("|") and not row.endswith("\\|"):
        row = row[:-1]
    return [c.strip().replace("\\|", "|") for c in re.split(r"(?<!\\)\|", row)]


def slug(text, seen):
    # GitHub anchor: drop markup and punctuation, lower-case, spaces to hyphens, -1/-2 for repeats.
    plain = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", text).replace("`", "").replace("*", "")
    base = re.sub(r"[^\w\- ]", "", plain.strip().lower()).replace(" ", "-")
    n = seen.get(base, 0)
    seen[base] = n + 1
    return base if n == 0 else f"{base}-{n}"


def indent_of(line):
    return len(line) - len(line.lstrip(" "))


def next_filled(lines, i):
    while i < len(lines) and not lines[i].strip():
        i += 1
    return i


def same_list(match, base, ordered):
    return bool(match) and len(match.group(1)) == base and match.group(2)[0].isdigit() == ordered


def list_item_body(lines, i, base, width):
    """Collects the lines indented under one list item; returns them dedented and the next index."""
    body = [lines[i][width:]]
    i += 1
    while i < len(lines):
        if lines[i].strip():
            if indent_of(lines[i]) <= base:
                break
            body.append(lines[i][min(width, indent_of(lines[i])):])
            i += 1
            continue
        j = next_filled(lines, i)
        if j >= len(lines) or indent_of(lines[j]) <= base:
            break
        body.extend([""] * (j - i))
        i = j
    return body, i


def render_list(lines, i, seen):
    first = LIST_ITEM.match(lines[i])
    base, ordered = len(first.group(1)), first.group(2)[0].isdigit()
    start = int(first.group(2)[:-1]) if ordered else 1
    items = []
    while i < len(lines):
        m = LIST_ITEM.match(lines[i])
        if not same_list(m, base, ordered):
            break
        body, i = list_item_body(lines, i, base, len(m.group(0)))
        # A tight item's first paragraph sits directly in the <li>.
        items.append(re.sub(r"^<p>(.*?)</p>", r"\1", render("\n".join(body), seen), count=1, flags=re.S))
        j = next_filled(lines, i)
        if j < len(lines) and same_list(LIST_ITEM.match(lines[j]), base, ordered):
            i = j
    tag = "ol" if ordered else "ul"
    attr = f' start="{start}"' if ordered and start != 1 else ""
    return f"<{tag}{attr}>" + "".join(f"<li>{item}</li>" for item in items) + f"</{tag}>", i


def render(src, seen=None):
    seen = {} if seen is None else seen
    lines = src.splitlines()
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if stripped.startswith("```"):
            code = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith("```"):
                code.append(lines[i])
                i += 1
            out.append("<pre><code>" + html.escape("\n".join(code)) + "</code></pre>")
        elif re.match(r"^#{1,6} ", stripped):
            level = len(stripped) - len(stripped.lstrip("#"))
            text = stripped[level + 1:]
            out.append(f'<h{level} id="{slug(text, seen)}">{inline(text)}</h{level}>')
        elif IMAGE.match(stripped):
            alt, src_url = IMAGE.match(stripped).groups()
            caption = f"<figcaption>{inline(alt)}</figcaption>" if alt else ""
            out.append(f'<figure><img src="{html.escape(src_url)}" alt="{html.escape(alt)}">{caption}</figure>')
        elif stripped.startswith("|") and i + 1 < len(lines) and re.match(r"^\|?\s*:?-{2,}", lines[i + 1].strip()):
            head = table_cells(line)
            out.append("<table><thead><tr>" + "".join(f"<th>{inline(c)}</th>" for c in head) + "</tr></thead><tbody>")
            i += 2
            while i < len(lines) and lines[i].strip().startswith("|"):
                out.append("<tr>" + "".join(f"<td>{inline(c)}</td>" for c in table_cells(lines[i])) + "</tr>")
                i += 1
            out.append("</tbody></table>")
            continue
        elif LIST_ITEM.match(line):
            block, i = render_list(lines, i, seen)
            out.append(block)
            continue
        elif stripped.startswith(">"):
            quote = []
            while i < len(lines) and lines[i].strip().startswith(">"):
                quote.append(re.sub(r"^> ?", "", lines[i].strip()))
                i += 1
            out.append("<blockquote>" + render("\n".join(quote), seen) + "</blockquote>")
            continue
        elif stripped in ("---", "***"):
            out.append("<hr>")
        elif stripped:
            para = [stripped]
            i += 1
            while i < len(lines) and lines[i].strip() and not BLOCK_START.match(lines[i].strip()):
                para.append(lines[i].strip())
                i += 1
            out.append(f"<p>{inline(' '.join(para))}</p>")
            continue
        i += 1
    return "\n".join(out)


def self_check():
    md = "# A b\n\n1. one\n\n   ```\n   x | y\n   ```\n\n   - sub\n2. two\n\n![Cap](a.png)\n\n9. nine\n\n| c |\n|---|\n| `a\\|b` |\n\n> **Tip** t\n> - q\n"
    got = render(md)
    assert '<h1 id="a-b">' in got
    assert "<ol><li>one\n<pre><code>x | y</code></pre>\n<ul><li>sub</li></ul></li><li>two</li></ol>" in got, got
    assert '<figure><img src="a.png" alt="Cap"><figcaption>Cap</figcaption></figure>' in got
    assert '<ol start="9">' in got and "<code>a|b</code>" in got
    assert "<blockquote><p><strong>Tip</strong> t</p>\n<ul><li>q</li></ul></blockquote>" in got, got
    assert inline("**`a*b`** *i*") == "<strong><code>a*b</code></strong> <em>i</em>"
    assert slug("18.6 이사·입주 청소 (견적)", {}) == "186-이사입주-청소-견적"


def main():
    if sys.argv[1:] == ["--self-check"]:
        self_check()
        return
    if len(sys.argv) == 4 and sys.argv[1] == "--fragment":
        dst_path = Path(sys.argv[3])
        dst_path.parent.mkdir(parents=True, exist_ok=True)
        dst_path.write_text(render(Path(sys.argv[2]).read_text(encoding="utf-8")) + "\n", encoding="utf-8")
        return
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    src_path, dst_path, title = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    body = render(src_path.read_text(encoding="utf-8"))
    page = (
        '<!doctype html><html lang="ko"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        f"<title>{html.escape(title)}</title><style>{CSS}</style></head><body>\n"
        f"{body}\n</body></html>\n"
    )
    dst_path.parent.mkdir(parents=True, exist_ok=True)
    dst_path.write_text(page, encoding="utf-8")


if __name__ == "__main__":
    main()
