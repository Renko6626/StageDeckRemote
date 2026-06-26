# StageDeck Remote · 技术白皮书

给合伙人去策划会用的 12-15 页技术白皮书 + 操作指南。

## 构建

需要 XeLaTeX + ctex + Noto CJK 字体。

```bash
# Ubuntu / Debian
sudo apt install texlive-xetex texlive-lang-chinese texlive-fonts-recommended fonts-noto-cjk

# 构建 PDF
cd docs/whitepaper
make           # 两遍跑,生成 main.pdf
make clean     # 删辅助文件,保留 PDF
make watch     # 文件变化时自动重编(需 latexmk)
```

## 结构

```
docs/whitepaper/
├── main.tex                  # 主文件,串起所有 section + 版本号宏
├── preamble.tex              # 字体 / 颜色 / 页眉 / 信息框样式
├── Makefile
├── README.md                 # 本文件
├── figures/                  # (用于实拍截图存放;TikZ 图都内联在 section 里)
└── sections/
    ├── 00-cover.tex          # 封面(含 A3 logo TikZ 重绘)
    ├── 01-summary.tex        # 提案摘要(策划会真正会看)
    ├── 02-scenarios.tex      # 4 个典型场景
    ├── 03-architecture.tex   # 架构总览 + TikZ 图 + 三层锁
    ├── 04-deployment.tex     # LAN / Relay 双模式对比
    ├── 05-operations.tex     # 演出从准备到收尾的操作指南
    ├── 06-security.tex       # 安全模型 + 已知限制 + 红线
    └── 99-appendix.tex       # 系统要求 / 协议简介 / 联系方式
```

## 编辑须知

- **版本号 / 日期**在 `main.tex` 顶部用 `\sdVersion` 和 `\sdDate` 一处定义,封面和页眉都引用它。每发版改一处即可。
- **截图位**目前是 `\screenshot{<高度>}{<说明>}` 命令画的虚线占位框。等在 Windows 上跑通 v0.2.0 之后,用真截图替换:
  1. 把 PNG 放到 `figures/`,起描述性名字(`gui-window-select.png` 之类);
  2. 把对应 `\screenshot{...}{...}` 换成 `\includegraphics[width=12cm]{figures/gui-window-select.png}`(需要在 preamble 里 `\usepackage{graphicx}`,目前已含)。
- **附录 C 联系方式**留了"〔此处填入合伙人联系方式〕"占位,投出去前替换。
- 调字号 / 页边距:`preamble.tex` 里 `geometry` 包参数。
- 改主色:`preamble.tex` 顶部的 `sdink / sdred / sdok / sdaccent` 几个 `\definecolor`。

## 给元火动漫社版本

封面"致 XXX"目前是泛用版。如果要做社团定制版,把 `sections/00-cover.tex` 里"舞台演出 PowerPoint 远程控制系统"下面加一行:

```latex
{\large\sffamily\color{sdaccent} 致元火动漫社}\\[3pt]
```

`01-summary.tex` 末尾"对元火动漫社的价值"小节已经是社团定向的,无需再改。
