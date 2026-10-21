#!/usr/bin/env python3
"""Count nonblank, non-comment Rust production lines under src/.

Pass --baseline <git-ref> to read that source tree with `git show`.
"""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path

CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")
TEST_MODULE = re.compile(
    r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;"
)


def skip_trivia(source: str, index: int) -> int:
    while index < len(source):
        if source[index].isspace():
            index += 1
        elif source.startswith("//", index):
            newline = source.find("\n", index)
            return len(source) if newline < 0 else skip_trivia(source, newline + 1)
        elif source.startswith("/*", index):
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
        else:
            break
    return index


def skip_quoted(source: str, index: int) -> int:
    """Skip a Rust string, raw string, byte string, or character literal."""
    start = index
    while index < len(source) and source[index] in "bcr":
        index += 1
    if index < len(source) and source[index] == "#":
        hash_start = index
        while index < len(source) and source[index] == "#":
            index += 1
        if index < len(source) and source[index] == '"':
            hashes = source[hash_start:index]
            end = source.find('"' + hashes, index + 1)
            return len(source) if end < 0 else end + 1 + len(hashes)
        index = start
    if index < len(source) and source[index] in "\"'":
        quote = source[index]
        # Apostrophes beginning lifetimes are not character literals.
        if quote == "'" and index + 1 < len(source) and (source[index + 1].isalpha() or source[index + 1] == "_"):
            return start
        index += 1
        while index < len(source):
            if source[index] == "\\":
                index += 2
            elif source[index] == quote:
                return index + 1
            else:
                index += 1
        return len(source)
    return start


def matching_delimiter(source: str, index: int, opening: str, closing: str) -> int:
    depth = 1
    index += 1
    while index < len(source) and depth:
        if source.startswith("//", index):
            newline = source.find("\n", index)
            index = len(source) if newline < 0 else newline + 1
        elif source.startswith("/*", index):
            index = skip_trivia(source, index)
        else:
            quoted_end = skip_quoted(source, index)
            if quoted_end != index:
                index = quoted_end
            elif source[index] == opening:
                depth += 1
                index += 1
            elif source[index] == closing:
                depth -= 1
                index += 1
            else:
                index += 1
    return index


def test_item_end(source: str, index: int) -> int:
    """Find the end of a cfg(test) item, including a braced body."""
    parens = brackets = 0
    while index < len(source):
        index = skip_trivia(source, index)
        if index >= len(source):
            return len(source)
        quoted_end = skip_quoted(source, index)
        if quoted_end != index:
            index = quoted_end
        elif source[index] == "(":
            parens += 1
            index += 1
        elif source[index] == ")":
            parens = max(0, parens - 1)
            index += 1
        elif source[index] == "[":
            brackets += 1
            index += 1
        elif source[index] == "]":
            brackets = max(0, brackets - 1)
            index += 1
        elif parens == 0 and brackets == 0 and source[index] == ";":
            return index + 1
        elif parens == 0 and brackets == 0 and source[index] == "{":
            end = matching_delimiter(source, index, "{", "}")
            end = skip_trivia(source, end)
            return end + 1 if end < len(source) and source[end] == ";" else end
        else:
            index += 1
    return len(source)


def remove_test_items(source: str) -> str:
    spans = []
    for match in CFG_TEST.finditer(source):
        end = test_item_end(source, match.end())
        spans.append((match.start(), end))
    for start, end in reversed(spans):
        source = source[:start] + "\n" * source[start:end].count("\n") + source[end:]
    return source


def is_comment_only(line: str) -> bool:
    stripped = line.strip()
    return not stripped or stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*") or stripped.startswith("*/")


def source_files(ref: str | None) -> list[tuple[str, str]]:
    if ref is None:
        return [(path.as_posix(), path.read_text()) for path in sorted(Path("src").rglob("*.rs"))]
    listing = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", ref, "--", "src"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    paths = [path for path in listing if path.endswith(".rs")]
    contents = []
    for path in paths:
        content = subprocess.run(
            ["git", "show", f"{ref}:{path}"],
            check=True,
            text=True,
            capture_output=True,
        ).stdout
        contents.append((path, content))
    return contents


def test_module_files(files: list[tuple[str, str]]) -> set[str]:
    """Return external module files declared only behind `#[cfg(test)]`."""
    excluded = set()
    available = {path for path, _ in files}
    for path, source in files:
        parent = Path(path).parent
        for module in TEST_MODULE.findall(source):
            for candidate in (parent / f"{module}.rs", parent / module / "mod.rs"):
                if candidate.as_posix() in available:
                    excluded.add(candidate.as_posix())
    return excluded


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", metavar="GIT_REF", help="read Rust sources from this git ref")
    args = parser.parse_args()

    total = 0
    files = source_files(args.baseline)
    excluded = test_module_files(files)
    for path, source in files:
        if path in excluded:
            continue
        production = remove_test_items(source)
        total += sum(not is_comment_only(line) for line in production.splitlines())
    origin = f"baseline {args.baseline}" if args.baseline else "working tree"
    print(f"Production Rust LOC ({origin}; {len(files)} files): {total}")


if __name__ == "__main__":
    main()
