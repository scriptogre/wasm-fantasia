"""Web build size report. Usage: python3 client/web_size.py [release]

Analyzes WASM binary sections and asset sizes for the web build.
No hardcoded project names — discovers everything from the filesystem.
"""

import glob, gzip, os, struct, sys

SECTION_NAMES = {
    0: "Custom", 1: "Type", 2: "Import", 3: "Function", 4: "Table",
    5: "Memory", 6: "Global", 7: "Export", 8: "Start", 9: "Element",
    10: "Code", 11: "Data", 12: "DataCount",
}
MB = 1024 * 1024
ASSETS_DIR = os.path.join(os.path.dirname(__file__), "assets")


def read_leb128(f):
    val = shift = 0
    while True:
        byte = f.read(1)[0]
        val |= (byte & 0x7F) << shift
        shift += 7
        if not (byte & 0x80):
            return val


def wasm_sections(path):
    """Parse WASM binary into {section_name: byte_size}."""
    sections = {}
    with open(path, "rb") as f:
        f.read(8)
        while b := f.read(1):
            sid, size = b[0], read_leb128(f)
            name = SECTION_NAMES.get(sid, f"?({sid})")
            if sid == 0:
                raw = f.read(read_leb128(f))
                cn = raw.decode("utf-8", errors="replace")
                f.seek(size - len(raw) - 1, 1)
                name = f"Custom({cn})"
            else:
                f.seek(size, 1)
            sections[name] = sections.get(name, 0) + size
    return sections


def find_wasm(pattern):
    """Find WASM files matching a glob pattern, return newest."""
    matches = glob.glob(pattern, recursive=True)
    bg = [m for m in matches if m.endswith("_bg.wasm")]
    if bg:
        return max(bg, key=os.path.getmtime)
    return max(matches, key=os.path.getmtime) if matches else None


def print_wasm(path, label):
    if not path or not os.path.exists(path):
        return
    sections = wasm_sections(path)
    total = sum(sections.values())
    print(f"\n  {label} ({total / MB:.1f} MB)")
    for name, size in sorted(sections.items(), key=lambda x: -x[1]):
        if size / total < 0.005:
            continue
        bar = "\u2588" * int(size * 30 / total)
        print(f"    {name:28s} {size / MB:6.1f} MB {bar}")
    gz = len(gzip.compress(open(path, "rb").read(), compresslevel=6))
    print(f"    {'gzip':28s} {gz / MB:6.1f} MB")


def dir_size(path):
    if not os.path.isdir(path):
        return 0
    return sum(
        os.path.getsize(os.path.join(dp, f))
        for dp, _, fns in os.walk(path) for f in fns
    )


def gz_file_size(path):
    return len(gzip.compress(open(path, "rb").read(), 6))


def main():
    do_release = "release" in sys.argv[1:]

    if do_release:
        print("Building release WASM...")
        ret = os.system(
            "cd client && rustup run nightly bevy build --yes "
            "--no-default-features --features web --release web "
            "-U multi-threading --bundle"
        )
        if ret != 0:
            sys.exit(1)

    print("\n" + "=" * 60)
    print("  WEB SIZE REPORT")
    print("=" * 60)

    # Dev WASM — bevy CLI puts these in target/wasm32-unknown-unknown/web/
    dev = find_wasm("target/wasm32-unknown-unknown/web/**/*.wasm")
    print_wasm(dev, "Dev WASM")

    # Release WASM — bevy CLI bundles to target/bevy_web/
    release = find_wasm("target/bevy_web/**/*.wasm")
    print_wasm(release, "Release WASM")

    # Assets
    print(f"\n  Assets")
    models_dir = os.path.join(ASSETS_DIR, "models")
    if os.path.isdir(models_dir):
        for f in sorted(os.listdir(models_dir)):
            if f.endswith((".glb", ".gltf")):
                p = os.path.join(models_dir, f)
                sz = os.path.getsize(p)
                gz = gz_file_size(p)
                print(f"    {f:28s} {sz / MB:5.1f} MB  (gzip {gz / MB:.1f} MB)")

    for name in ["audio", "fonts", "textures", "shaders"]:
        sz = dir_size(os.path.join(ASSETS_DIR, name))
        if sz > 1024:
            print(f"    {name + '/':28s} {sz / MB:5.1f} MB")


if __name__ == "__main__":
    main()
