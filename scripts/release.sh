#!/usr/bin/env bash
# StageDeck Remote · release 自动化
#
# 用法:
#   scripts/release.sh <new-version> [flags]
# 示例:
#   scripts/release.sh 0.2.1
#   scripts/release.sh 0.2.1 --no-relay --no-tag
#   scripts/release.sh 0.2.1 --skip-tests           # 仅在紧急修复时
#
# 做的事:
#   1. 校验工作树干净、在 master、版本号合法
#   2. 更新 4 处版本号 (package.json / src-tauri/Cargo.toml /
#                       src-tauri/tauri.conf.json / 根 Cargo.lock)
#   3. cargo test --workspace + cargo xwin build (Windows 目标编译)
#   4. pnpm tauri build 出 NSIS 安装包
#   5. (可选) cargo build --release -p relay 出 Linux 二进制
#   6. 把产物 cp 到 out/
#   7. commit "chore: bump version X → Y"
#   8. (默认) tag v<new-version>
#
# 环境变量(可覆盖):
#   TAURI_DEV   conda env 路径(native build / cargo test)
#   XWIN_BIN    conda env bin 路径(cargo-xwin / Windows 交叉编译)
#
# 退出码:
#   0  OK   ·  非 0  任一步失败(会指明哪一步)

set -euo pipefail

# ── 配色(只在 tty 时启用)─────────────────────────────────────
if [[ -t 1 ]]; then
  C_INFO='\033[36m'; C_OK='\033[32m'; C_WARN='\033[33m'; C_ERR='\033[31m'; C_RST='\033[0m'
else
  C_INFO=''; C_OK=''; C_WARN=''; C_ERR=''; C_RST=''
fi
say()  { printf "${C_INFO}==>${C_RST} %s\n" "$*"; }
ok()   { printf "${C_OK}✓${C_RST} %s\n" "$*"; }
warn() { printf "${C_WARN}!${C_RST} %s\n" "$*" >&2; }
die()  { printf "${C_ERR}✗ %s${C_RST}\n" "$*" >&2; exit 1; }

# ── 参数解析 ──────────────────────────────────────────────
NEW_VERSION=""
DO_TAG=1
DO_RELAY=1
DO_TESTS=1
DO_INSTALLER=1
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'; exit 0
      ;;
    --no-tag)       DO_TAG=0; shift ;;
    --no-relay)     DO_RELAY=0; shift ;;
    --no-installer) DO_INSTALLER=0; shift ;;
    --skip-tests)   DO_TESTS=0; shift ;;
    --dry-run)      DRY_RUN=1; shift ;;
    -*)             die "未知参数: $1(--help 看用法)" ;;
    *)
      if [[ -n "$NEW_VERSION" ]]; then die "只能指定一个版本号(已收到: $NEW_VERSION)"; fi
      NEW_VERSION="$1"; shift
      ;;
  esac
done

[[ -n "$NEW_VERSION" ]] || die "缺少 <new-version>(--help 看用法)"
[[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "版本号格式不对: $NEW_VERSION(应形如 0.2.1)"

# ── 环境与路径 ────────────────────────────────────────────
REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
TAURI_DEV="${TAURI_DEV:-/data/sunyunbo/miniconda3/envs/tauri-dev}"
XWIN_BIN="${XWIN_BIN:-/data/sunyunbo/miniconda3/envs/xwin/bin}"

# ── 0. 前置检查 ────────────────────────────────────────────
say "前置检查"
[[ -d .git ]] || die "不在 git 仓库根目录"
BRANCH=$(git rev-parse --abbrev-ref HEAD)
[[ "$BRANCH" == "master" ]] || warn "当前不在 master,而是 $BRANCH(允许,但 release 通常从 master 出)"
if ! git diff --quiet || ! git diff --cached --quiet; then
  die "工作树不干净;先 commit 或 stash 你的改动"
fi
[[ -x "$XWIN_BIN/cargo-xwin" || -x "$XWIN_BIN/cargo-xwin" ]] || warn "XWIN_BIN=$XWIN_BIN 下没看到 cargo-xwin;交叉编译会失败"
ok "工作树干净,分支 $BRANCH"

# ── 1. 取当前版本 + 自检 ────────────────────────────────────
CURR_VERSION=$(sed -n '4s/.*"version": "\([^"]*\)".*/\1/p' package.json)
[[ -n "$CURR_VERSION" ]] || die "解析不出 package.json 的当前版本"
say "当前版本: $CURR_VERSION  →  新版本: $NEW_VERSION"
[[ "$CURR_VERSION" == "$NEW_VERSION" ]] && die "新版本等于当前版本,不会前进"

# 确认 4 个文件当前都一致(防止上次半途崩了留下歪斜)
A=$(sed -n '4s/.*"version": "\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json)
B=$(sed -n '3s/^version = "\([^"]*\)"$/\1/p' src-tauri/Cargo.toml)
LL_LINE=$(grep -n 'name = "stagedeck-remote"' Cargo.lock | head -1 | cut -d: -f1)
[[ -n "$LL_LINE" ]] || die "Cargo.lock 里找不到 stagedeck-remote 条目"
C=$(sed -n "$((LL_LINE+1))s/^version = \"\([^\"]*\)\"$/\1/p" Cargo.lock)
if [[ "$A" != "$CURR_VERSION" || "$B" != "$CURR_VERSION" || "$C" != "$CURR_VERSION" ]]; then
  die "四处版本号不一致 — package=$CURR_VERSION tauri.conf=$A Cargo.toml=$B Cargo.lock=$C(先手工对齐再 release)"
fi
ok "四处版本号一致 = $CURR_VERSION"

if (( DRY_RUN )); then
  say "DRY-RUN: 后面只打印不执行"
fi

run() {
  if (( DRY_RUN )); then echo "  [dry-run] $*"; else "$@"; fi
}

# ── 2. 改 4 个文件 ─────────────────────────────────────────
say "更新版本号"
run sed -i "4s/\"$CURR_VERSION\"/\"$NEW_VERSION\"/" package.json
run sed -i "3s/$CURR_VERSION/$NEW_VERSION/" src-tauri/Cargo.toml
run sed -i "4s/\"$CURR_VERSION\"/\"$NEW_VERSION\"/" src-tauri/tauri.conf.json
run sed -i "$((LL_LINE+1))s/$CURR_VERSION/$NEW_VERSION/" Cargo.lock
ok "4 处版本号 → $NEW_VERSION"

# ── 3. 测试 ────────────────────────────────────────────────
if (( DO_TESTS )); then
  say "cargo test --workspace"
  if (( DRY_RUN )); then echo "  [dry-run]"; else
    PKG_CONFIG_PATH="$TAURI_DEV/lib/pkgconfig:$TAURI_DEV/share/pkgconfig" \
    LD_LIBRARY_PATH="$TAURI_DEV/lib" \
    PATH="$TAURI_DEV/bin:$PATH" \
      cargo test --workspace --quiet 2>&1 | tail -5
  fi
  ok "测试通过"
else
  warn "--skip-tests 跳过测试(仅紧急用)"
fi

# ── 4. Windows 交叉编译(出 installer)──────────────────────
if (( DO_INSTALLER )); then
  say "Windows 安装包(NSIS,经 cargo-xwin)"
  if (( DRY_RUN )); then echo "  [dry-run]"; else
    XWIN_ACCEPT_LICENSE=1 PATH="$XWIN_BIN:$PATH" \
      pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis 2>&1 | tail -3
  fi
  INSTALLER="target/x86_64-pc-windows-msvc/release/bundle/nsis/StageDeck Remote_${NEW_VERSION}_x64-setup.exe"
  if (( DRY_RUN == 0 )); then
    [[ -f "$INSTALLER" ]] || die "构建后找不到 $INSTALLER"
    mkdir -p out
    cp "$INSTALLER" out/
    ok "$INSTALLER → out/"
  fi
else
  warn "--no-installer 跳过 Windows 安装包构建"
fi

# ── 5. Linux relay 二进制 ──────────────────────────────────
if (( DO_RELAY )); then
  say "relay binary(Linux x86_64)"
  if (( DRY_RUN )); then echo "  [dry-run]"; else
    cargo build --release -p relay --quiet 2>&1 | tail -3
  fi
  RELAY_VER=$(sed -n '3s/^version = "\([^"]*\)"$/\1/p' crates/relay/Cargo.toml)
  RELAY_OUT="out/stagedeck-relay-linux-x86_64-${RELAY_VER}"
  if (( DRY_RUN == 0 )); then
    [[ -f target/release/relay ]] || die "构建后找不到 target/release/relay"
    mkdir -p out
    cp target/release/relay "$RELAY_OUT"
    ok "$RELAY_OUT(relay 版本 $RELAY_VER;主 app 版本 $NEW_VERSION)"
  fi
else
  warn "--no-relay 跳过 relay 二进制构建"
fi

# ── 6. commit + tag ──────────────────────────────────────
say "commit + tag"
run git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json Cargo.lock
run git commit -q -m "chore: bump version $CURR_VERSION → $NEW_VERSION"
ok "commit"

if (( DO_TAG )); then
  if git rev-parse "v$NEW_VERSION" >/dev/null 2>&1; then
    warn "tag v$NEW_VERSION 已存在,跳过"
  else
    run git tag -a "v$NEW_VERSION" -m "v$NEW_VERSION"
    ok "tag v$NEW_VERSION"
  fi
else
  warn "--no-tag 跳过 tag"
fi

# ── 7. 提示 ─────────────────────────────────────────────────
echo ""
say "下一步建议"
echo "  · 改 CHANGELOG.md(脚本没自动写)"
echo "  · 验产物: ls -la out/"
echo "  · 推远端: git push && git push --tags"
ok "release $NEW_VERSION 完成"
