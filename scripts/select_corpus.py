import argparse
from pathlib import Path

from campuslingo_ai.corpus import scan_corpus
from campuslingo_ai.reporting import utc_now, write_report

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("/home/mahdi/UTM_eLearning"))
    args = parser.parse_args()
    report = {"observed_at": utc_now(), **scan_corpus(args.root)}
    path = write_report("private/corpus.json", report)
    for category, record in report["selected"].items():
        print(f"{category}: {Path(record['path']).name} ({record['pages']} pages; {record['language_evidence']})")
    print(f"Private manifest: {path}")
