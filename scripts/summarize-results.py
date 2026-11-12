#!/usr/bin/env python3
"""Roll up results/docker/*/report/summary.json into results/SUMMARY.md (and print it)."""
import json, sys
from collections import Counter
from pathlib import Path

root = Path(__file__).resolve().parent.parent
docker = root / "results" / "docker"
EXCLUDE = {"test_files"}  # fixture runs, not real repos

rows, vacuous = [], []
for d in sorted(docker.iterdir()):
    f = d / "report" / "summary.json"
    if d.name in EXCLUDE or not f.exists():
        continue
    entries = json.loads(f.read_text())
    c = Counter(e["verdict"].split(":")[0] for e in entries if "verdict" in e)  # "inconclusive: <reason>"
    skipped = [e["skipped"] for e in entries if "skipped" in e]
    mutated = c["vacuous"] + c["not_vacuous"] + c["inconclusive"]
    status = "skipped: " + skipped[0][:60] if skipped and not mutated else "ok"
    rows.append((d.name, mutated, c["vacuous"], c["not_vacuous"], c["inconclusive"], c["not_compiled"], status))
    vacuous += [(d.name, e["file"], e["line"], e["function"]) for e in entries if e.get("verdict", "").startswith("vacuous")]

out = ["# Vacuity results", "",
       "| Repo | Mutated | Vacuous | Not vacuous | Inconclusive | Not compiled | Status |",
       "|---|---|---|---|---|---|---|"]
for r in rows:
    out.append("| " + " | ".join(map(str, r)) + " |")
tot = [sum(r[i] for r in rows) for i in range(1, 6)]
out.append("| **Total** | " + " | ".join(f"**{t}**" for t in tot) + " | |")
out += ["", "## Vacuous functions", ""]
out += [f"- `{r}`: `{f}:{l}` {fn}" for r, f, l, fn in vacuous] or ["None found."]
text = "\n".join(out) + "\n"
(root / "results" / "SUMMARY.md").write_text(text)
print(text)
