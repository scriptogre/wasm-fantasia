"""Check the laws and confirm that broken implementations are rejected."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--verus", default="verus", help="Verus executable")
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
verus = shutil.which(args.verus)
if not verus:
    parser.error("Verus not found; pass --verus /path/to/verus")

subprocess.run([verus, str(root / "verification/verify.rs"), "--no-cheating"], check=True)
mutations = [
    ("cap", "src/fury.rs", "stacks > 12", "stacks > 13", "postcondition not satisfied"),
    ("critical gain", "src/fury.rs", "is_crit { 3 }", "is_crit { 2 }", "postcondition not satisfied"),
    ("bonus", "src/fury.rs", "bounded(stacks) * 12", "bounded(stacks) * 13", "postcondition not satisfied"),
    ("refresh", "src/fury.rs", "remaining_micros: 2_500_000", "remaining_micros: 2_000_000", "postcondition not satisfied"),
    ("expiry", "src/fury.rs", "micros >= remaining", "micros > remaining", "postcondition not satisfied"),
    ("input domain", "src/fury.rs", "verus_spec(returns on_hit", "verus_spec(requires stacks >= 0, returns on_hit", "precondition not satisfied"),
    ("missing proof", "verification/proof.rs", "proof fn bonus(stacks: i64) {}", "", "not all trait items implemented"),
    ("assumed proof", "verification/proof.rs", "proof fn bonus(stacks: i64) {}", "proof fn bonus(stacks: i64) { assume(false); }", "not allowed"),
]
with tempfile.TemporaryDirectory(prefix="fury-laws-") as directory:
    scratch = Path(directory)
    for name, file, before, after, diagnostic in mutations:
        shutil.copytree(root / "verification", scratch / "verification", dirs_exist_ok=True)
        (scratch / "src").mkdir(exist_ok=True)
        shutil.copyfile(root / "src/fury.rs", scratch / "src/fury.rs")
        target = scratch / file
        original = target.read_text()
        if before not in original:
            raise RuntimeError(f"Mutation no longer applies: {name}")
        target.write_text(original.replace(before, after, 1))
        result = subprocess.run(
            [verus, str(scratch / "verification/verify.rs"), "--no-cheating"],
            capture_output=True, text=True, timeout=60,
        )
        if result.returncode == 0 or diagnostic not in result.stdout + result.stderr:
            raise RuntimeError(f"Unexpected result for {name}:\n{result.stdout}\n{result.stderr}")
        print(f"Rejected: {name}")
