#!/usr/bin/env bash
# 从 CHANGELOG.md 抽出指定版本的 section,打到 stdout。
#
# 用法:  scripts/extract-changelog.sh 0.2.1
#
# 匹配规则:从 "## [<version>]" 这一行开始(含 " — 日期" 之类的后缀都吃),
# 直到下一个 "## [" 之前。"## [0.1.0 – 0.1.12]" 这类范围标题也会被认成
# 下一个 section 的开头(因为它也是 "## [" 开头),所以适合做边界。
#
# 退出码:  0  找到非空内容  ·  非 0  没有匹配 / 匹配为空(workflow 会回退)

set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
CHANGELOG="${CHANGELOG_FILE:-CHANGELOG.md}"

[[ -f "$CHANGELOG" ]] || { echo "no $CHANGELOG" >&2; exit 1; }

awk -v ver="$VERSION" '
  BEGIN { capturing = 0; have_content = 0 }
  # 命中本版本 section 起点(允许有 — 日期 后缀)
  match($0, /^## \[([^]]+)\]/, m) {
    if (capturing == 1) { exit }
    if (m[1] == ver) { capturing = 1; print; next }
  }
  capturing == 1 {
    print
    # 任何非空行算作有内容
    if ($0 ~ /[^[:space:]]/) { have_content = 1 }
  }
  END {
    if (capturing == 0 || have_content == 0) { exit 1 }
  }
' "$CHANGELOG"
