#!/bin/sh
# Offline validation for this Qoder plugin package, plus the mirror drift check
# against docs/rxrust-source-analysis.
#
# Why not the bundled create-plugin validator (validate_qoder_plugin.py): this
# repository bans Python (see .qoder/rules/rust.md), so the equivalent checks are
# re-implemented here in POSIX shell + Node for JSON parsing.
#
# Usage:  sh scripts/validate.sh [plugin-root]     (default: this package)
# Requires: sh, find, diff, node (for JSON parsing only).

set -eu

ROOT=${1:-"$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"}
MANIFEST="$ROOT/.qoder-plugin/plugin.json"
SKILLS_DIR="$ROOT/skills"
SRC_DOCS="$(CDPATH= cd -- "$ROOT/../.." && pwd)/docs/rxrust-source-analysis"
MIRROR="$SKILLS_DIR/rxrust-source-analysis/references"

errors=0
tmp=$(mktemp "${TMPDIR:-/tmp}/qoder-plugin-validate.XXXXXX")
trap 'rm -f "$tmp"' EXIT INT TERM
fail() { printf 'ERROR: %s\n' "$1"; errors=$((errors + 1)); }
note() { printf 'ok: %s\n' "$1"; }

# 1. manifest exists, is valid JSON, is an object with non-empty name/version
[ -f "$MANIFEST" ] || fail "missing manifest: $MANIFEST"
if [ -f "$MANIFEST" ]; then
  node -e '
    const fs = require("fs");
    const err = (m) => console.log("ERROR: " + m);
    const warn = (m) => console.log("WARNING: " + m);
    let m;
    try { m = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); }
    catch (e) { err("plugin manifest is invalid JSON: " + e.message); process.exit(0); }
    if (m === null || typeof m !== "object" || Array.isArray(m)) err("plugin manifest must be a JSON object");
    else for (const f of ["name", "version"]) {
      if (typeof m[f] !== "string" || m[f].trim() === "") err("plugin.json missing required string field: " + f);
    }
    if (m && typeof m.name === "string" && !/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(m.name))
      warn("plugin name may not be portable: " + m.name);
    const pathFields = ["skills", "commands", "mcpServers", "hooks", "rules", "agents", "logo"];
    for (const f of pathFields) {
      const v = m && m[f];
      if (v === undefined) continue;
      const list = Array.isArray(v) ? v : (typeof v === "string" ? [v] : Object.values(v).map(x => (typeof x === "string" ? x : x && x.source)));
      for (const p of list) {
        if (typeof p !== "string" || p === "") { err(f + ": empty or non-string path"); continue; }
        if (/^https?:\/\//.test(p)) { warn(f + " uses remote URL; prefer copying assets into the plugin: " + p); continue; }
        if (!p.startsWith("./")) err(f + " path must start with ./: " + p);
        const rel = p.replace(/^\.\//, "");
        if (rel === "" || rel.startsWith("/") || rel.split("/").includes(".."))
          err(f + " path must stay inside plugin root: " + p);
        if ((f === "mcpServers" || f === "hooks") && !rel.endsWith(".json"))
          err(f + " path should end with .json: " + p);
      }
    }
  ' "$MANIFEST" > "$tmp" 2>&1 || true
  if [ -s "$tmp" ]; then
    while IFS= read -r line; do
      case "$line" in
        WARNING:*) printf 'warn: %s\n' "${line#WARNING: }" ;;
        ERROR:*) fail "${line#ERROR: }" ;;
        *) fail "$line" ;;
      esac
    done < "$tmp"
  fi
  note "manifest parsed: $(basename "$ROOT")"
fi

# 2. plugin root name equals manifest name
if [ -f "$MANIFEST" ]; then
  mname=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).name)' "$MANIFEST")
  [ "$(basename "$ROOT")" = "$mname" ] || fail "root dir '$(basename "$ROOT")' != manifest name '$mname'"
fi

# 3. declared component paths exist
for field in skills rules agents logo; do
  [ -f "$MANIFEST" ] || continue
  val=$(node -e 'const m=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));const v=m[process.argv[2]];if(typeof v==="string")console.log(v)' "$MANIFEST" "$field")
  [ -n "$val" ] || continue
  [ -e "$ROOT/${val#./}" ] || fail "declared $field path does not exist: $val"
done

# 4. every skill has frontmatter with name + description (<=1024 chars total),
#    and every relative markdown link in it resolves to a real file
found_skills=0
if [ -d "$SKILLS_DIR" ]; then
  for skill in "$SKILLS_DIR"/*/SKILL.md; do
    [ -f "$skill" ] || continue
    found_skills=$((found_skills + 1))
    base=$(dirname "$skill")
    fm=$(awk 'NR==1 && $0=="---" {f=1;next} f && $0=="---" {exit} f' "$skill")
    [ -n "$fm" ] || { fail "skill is missing YAML frontmatter: $skill"; continue; }
    fmchars=$(printf '%s' "$fm" | wc -m | tr -d ' ')
    [ "$fmchars" -le 1024 ] || fail "frontmatter exceeds 1024 chars ($fmchars): $skill"
    sname=$(printf '%s\n' "$fm" | sed -n 's/^name:[[:space:]]*//p' | head -1 | tr -d '"')
    sdesc=$(printf '%s\n' "$fm" | sed -n 's/^description:[[:space:]]*//p' | head -1 | tr -d '"')
    [ -n "$sname" ] || fail "skill frontmatter missing name: $skill"
    [ -n "$sdesc" ] || fail "skill frontmatter missing description: $skill"
    case "$sname" in
      *[!A-Za-z0-9-]* | "") [ -n "$sname" ] && fail "skill name must be letters/numbers/hyphens only: $sname" ;;
    esac
    [ "$sname" = "$(basename "$base")" ] || \
      printf 'warn: skill name "%s" != folder "%s"\n' "$sname" "$(basename "$base")"
    # relative links (skip URLs, anchors, absolute paths) must resolve
    links=$(grep -o ']([^)[:space:]]*)' "$skill" | sed 's/^](//; s/)$//; s/#.*$//' \
            | grep -v '^https\?:' | grep -v '^/' | sort -u || true)
    for rel in $links; do
      [ -e "$base/$rel" ] || fail "SKILL.md link target missing: $skill -> $rel"
    done
  done
fi
[ "$found_skills" -gt 0 ] || fail "declared skills directory contains no SKILL.md: $SKILLS_DIR"

# 6. forbidden content: other-runtime manifests and local state
for dir in .codex-plugin .cursor-plugin .claude-plugin; do
  [ -e "$ROOT/$dir" ] && fail "non-Qoder manifest directory present: $dir"
done
bad=$(find "$ROOT" \( -name '.DS_Store' -o -name '__MACOSX' -o -name 'settings.local.json' \) -print 2>/dev/null || true)
[ -n "$bad" ] && fail "local or generated state must not be packaged: $bad"

# 7. mirror fidelity against docs/rxrust-source-analysis (source of truth)
if [ -d "$SRC_DOCS" ]; then
  if diff -rq "$SRC_DOCS" "$MIRROR" >/dev/null 2>&1; then
    note "references/ is a byte-identical mirror of docs/rxrust-source-analysis"
  else
    diff -rq "$SRC_DOCS" "$MIRROR" || true
    fail "mirror drift: run  cp $SRC_DOCS/*.md $MIRROR/"
  fi
else
  printf 'warn: source dir not present (%s); mirror check skipped\n' "$SRC_DOCS"
fi

# 8. an installed project copy under .qoder/skills must match the packaged skill
WORKSPACE="$(CDPATH= cd -- "$ROOT/../.." && pwd)"
if [ -d "$SKILLS_DIR" ]; then
  for skill_dir in "$SKILLS_DIR"/*/; do
    [ -d "$skill_dir" ] || continue
    sname=$(basename "$skill_dir")
    installed="$WORKSPACE/.qoder/skills/$sname"
    if [ -d "$installed" ]; then
      if diff -rq "${skill_dir%/}" "$installed" >/dev/null 2>&1; then
        note "installed copy matches package: .qoder/skills/$sname"
      else
        diff -rq "${skill_dir%/}" "$installed" || true
        fail "installed copy drifted: run  rm -rf .qoder/skills/$sname && cp -R '${skill_dir%/}' .qoder/skills/"
      fi
    else
      printf 'warn: .qoder/skills/%s absent; install check skipped\n' "$sname"
    fi
  done
fi

if [ "$errors" -eq 0 ]; then
  printf 'OK: no issues found (%d skill)\n' "$found_skills"
  exit 0
fi
printf 'FAILED: %d issue(s)\n' "$errors"
exit 1
