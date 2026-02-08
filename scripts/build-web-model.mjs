#!/usr/bin/env node
/**
 * Generates player-web.glb from player.glb by:
 * 1. Parsing the Animation enum from Rust source to get required clip names
 * 2. Removing all animations not in that list
 * 3. Applying quantize + prune optimizations
 *
 * Usage:
 *   node scripts/build-web-model.mjs           # regenerate player-web.glb
 *   node scripts/build-web-model.mjs --verify  # check player-web.glb has all required clips
 */

import { readFileSync } from "fs";
import { NodeIO } from "@gltf-transform/core";
import { quantize, prune } from "@gltf-transform/functions";

const ANIMATION_RS = "client/src/player/animation.rs";
const PLAYER_GLB = "client/assets/models/player.glb";
const OUTPUT_GLB = "client/assets/models/player-web.glb";

/** Parse clip names from Animation::clip_name() match arms in Rust source. */
function parseClipNames(rustSource) {
  // Match lines like: Self::Idle => "Idle_Loop",
  const re = /Self::\w+\s*=>\s*"([^"]+)"/g;

  // Only look inside the clip_name() function
  const fnStart = rustSource.indexOf("fn clip_name(self)");
  if (fnStart === -1) {
    throw new Error(`Could not find clip_name() in ${ANIMATION_RS}`);
  }

  // Find the closing brace of the match block
  let braceDepth = 0;
  let fnEnd = fnStart;
  let started = false;
  for (let i = fnStart; i < rustSource.length; i++) {
    if (rustSource[i] === "{") {
      braceDepth++;
      started = true;
    } else if (rustSource[i] === "}") {
      braceDepth--;
      if (started && braceDepth === 0) {
        fnEnd = i;
        break;
      }
    }
  }

  const fnBody = rustSource.slice(fnStart, fnEnd + 1);
  const names = [];
  let m;
  while ((m = re.exec(fnBody)) !== null) {
    names.push(m[1]);
  }

  if (names.length === 0) {
    throw new Error("Found clip_name() but extracted zero clip names");
  }

  return names;
}

async function main() {
  const verify = process.argv.includes("--verify");
  const rustSource = readFileSync(ANIMATION_RS, "utf-8");
  const requiredClips = parseClipNames(rustSource);

  console.log(
    `Parsed ${requiredClips.length} required clips from Animation enum`
  );

  const io = new NodeIO();

  if (verify) {
    // Verify mode: check that player-web.glb contains all required clips
    const doc = await io.read(OUTPUT_GLB);
    const present = new Set(doc.getRoot().listAnimations().map((a) => a.getName()));
    let missing = requiredClips.filter((name) => !present.has(name));

    if (missing.length > 0) {
      console.error(`FAIL: player-web.glb is missing animations: ${missing.join(", ")}`);
      console.error(`Run 'just web-model' to regenerate.`);
      process.exit(1);
    }

    console.log(
      `OK: player-web.glb contains all ${requiredClips.length} required clips`
    );
    return;
  }

  // Build mode: generate player-web.glb
  const doc = await io.read(PLAYER_GLB);
  const root = doc.getRoot();
  const required = new Set(requiredClips);

  // Remove unused animations
  let removed = 0;
  for (const anim of root.listAnimations()) {
    if (!required.has(anim.getName())) {
      anim.dispose();
      removed++;
    }
  }

  const kept = root.listAnimations().length;
  console.log(`Kept ${kept} animations, removed ${removed}`);

  // Optimize
  await doc.transform(quantize(), prune());

  await io.write(OUTPUT_GLB, doc);
  console.log(`Wrote ${OUTPUT_GLB}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
