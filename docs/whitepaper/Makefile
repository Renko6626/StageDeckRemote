# StageDeck Remote 白皮书 · 构建脚本
#
# 需要:xelatex(TeX Live full 或 BasicTeX + ctex + Noto CJK 字体)
#
# 常用目标:
#   make            构建 main.pdf(默认目标,两遍跑全 TOC + 引用)
#   make once       只跑一遍(快速预览,TOC 可能不对)
#   make clean      删辅助文件,保留 PDF
#   make distclean  删辅助文件 + PDF
#   make watch      文件变化时重编(需 latexmk + inotifywait,可选)

LATEX     = xelatex
LATEXOPTS = -interaction=nonstopmode -halt-on-error
MAIN      = main
PDF       = $(MAIN).pdf
SOURCES   = $(MAIN).tex preamble.tex $(wildcard sections/*.tex)

.PHONY: all once clean distclean watch

all: $(PDF)

# 两遍跑:第一遍生成 .aux/.toc,第二遍把目录页和交叉引用解析对
$(PDF): $(SOURCES)
	$(LATEX) $(LATEXOPTS) $(MAIN).tex
	$(LATEX) $(LATEXOPTS) $(MAIN).tex

once: $(SOURCES)
	$(LATEX) $(LATEXOPTS) $(MAIN).tex

clean:
	rm -f *.aux *.log *.toc *.out *.fls *.fdb_latexmk *.synctex.gz
	rm -f sections/*.aux

distclean: clean
	rm -f $(PDF)

watch:
	@which latexmk >/dev/null 2>&1 || { echo "需要 latexmk;sudo apt install latexmk"; exit 1; }
	latexmk -pvc -xelatex $(MAIN).tex
