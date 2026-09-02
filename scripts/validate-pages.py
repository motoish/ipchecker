#!/usr/bin/env python3

from html.parser import HTMLParser
from pathlib import Path
from sys import argv
from urllib.parse import urlsplit


STABLE_RELEASE_URL = "https://github.com/motoish/ipchecker/releases/latest"
ROOT_REPOSITORY_URL = "https://github.com/motoish/ipchecker"


class PageDocument(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.ids: set[str] = set()
        self.references: list[str] = []
        self.downloads: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if element_id := values.get("id"):
            self.ids.add(element_id)

        for attribute in ("href", "src"):
            if reference := values.get(attribute):
                self.references.append(reference)

        if tag == "a" and "data-primary-download" in values:
            self.downloads.append(values.get("href", ""))


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    required_files = (
        root / "index.html",
        root / "styles.css",
        root / ".nojekyll",
        root / "assets/ipchecker-icon.png",
        root / "assets/og.png",
    )
    for path in required_files:
        if not path.is_file():
            errors.append(f"missing required file: {path}")

    index_path = root / "index.html"
    if not index_path.is_file():
        return errors

    document = PageDocument()
    document.feed(index_path.read_text(encoding="utf-8"))

    for reference in document.references:
        if reference.startswith("#"):
            if reference[1:] not in document.ids:
                errors.append(f"missing fragment target: {reference}")
            continue

        parsed = urlsplit(reference)
        if parsed.scheme or reference.startswith("mailto:"):
            continue
        if parsed.path.startswith("/"):
            errors.append(f"root-relative path breaks the /ipchecker/ site: {reference}")
            continue
        if parsed.path and not (root / parsed.path).is_file():
            errors.append(f"missing local reference: {reference}")

    if len(document.downloads) != 1:
        errors.append(f"expected 1 primary download link, found {len(document.downloads)}")
    for download in document.downloads:
        if download != STABLE_RELEASE_URL:
            errors.append(f"primary download does not target the stable release: {download}")

    root_repository_links = document.references.count(ROOT_REPOSITORY_URL)
    if root_repository_links != 1:
        errors.append(
            f"expected 1 root repository link, found {root_repository_links}"
        )

    return errors


def main() -> int:
    root = Path(argv[1]) if len(argv) == 2 else Path("docs/pages")
    errors = validate(root)
    if errors:
        for error in errors:
            print(f"error: {error}")
        return 1

    print(f"validated GitHub Pages site at {root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
