#!/usr/bin/env python3
"""Prepare the audited quiz template copy and sound files; requires only stdlib."""
import argparse
import copy
import hashlib
import io
import json
import posixpath
import wave
from pathlib import Path
from xml.etree import ElementTree as ET
from zipfile import ZIP_DEFLATED, ZipFile


ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA256 = "b44a19f91ac6910cff31246120fa0ecb98b4836991a86dba720d8a1258b6763f"
NS = {
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
}
UNRESOLVED = (
    "第4页 back 仍绑定不存在的 ResetGame；须确认撤销最近一条还是清空本页。"
    "跨页重置、多重连线、撤销规则和模块合并未修改。尚未在 Windows PowerPoint 验证。"
)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def prepare(source, output):
    source = source.resolve(strict=True)
    output = output.resolve()
    bundle = output.with_name(output.name + ".zip")
    require(not source.is_relative_to(output) and source != bundle, "输出不得覆盖输入文件")
    require(not output.exists() and not bundle.exists(), "输出目录或 ZIP 已存在，请另选 --output")
    original = source.read_bytes()
    require(sha256(original) == SOURCE_SHA256, "输入不是已审查的模板版本；停止修改，请先重新审查")
    candidate = io.BytesIO()
    sounds = {}
    sound_info = {}
    with ZipFile(io.BytesIO(original)) as src, ZipFile(candidate, "w") as dst:
        require(src.testzip() is None, "原件 ZIP CRC 校验失败")
        for entry in src.infolist():
            data = src.read(entry.filename)
            if entry.filename == "ppt/slides/slide5.xml":
                old = b"ppaction://macro?name=Click_Back"
                new = "ppaction://macro?name=模块3.Click_Back".encode("utf-8")
                # Shape click action and the text-run click action must agree.
                require(data.count(old) == 2, "第5页 back 绑定数量与审查结果不符")
                data = data.replace(old, new)
                ET.fromstring(data)
            dst.writestr(copy.copy(entry), data)

        rels = {r.attrib["Id"]: r.attrib for r in ET.fromstring(
            src.read("ppt/slides/_rels/slide2.xml.rels"))}
        slide = ET.fromstring(src.read("ppt/slides/slide2.xml"))
        for node in slide.findall(".//p:sndTgt", NS):
            name = node.attrib["name"]
            if name not in ("SE_Miss.wav", "SE_Bingo.wav"):
                continue
            rid = node.attrib["{" + NS["r"] + "}embed"]
            entry = posixpath.normpath(posixpath.join("ppt/slides", rels[rid]["Target"]))
            data = src.read(entry)
            require(name not in sounds or sounds[name] == data, "同名音效内容不一致")
            sounds[name] = data
            with wave.open(io.BytesIO(data)) as wav:
                sound_info[name] = {
                    "source_entry": entry, "sha256": sha256(data), "bytes": len(data),
                    "seconds": wav.getnframes() / wav.getframerate(),
                }
        require(set(sounds) == {"SE_Miss.wav", "SE_Bingo.wav"}, "未找到两份预期音效")

    candidate_bytes = candidate.getvalue()
    with ZipFile(io.BytesIO(original)) as src, ZipFile(io.BytesIO(candidate_bytes)) as dst:
        require(dst.testzip() is None, "候选 ZIP CRC 校验失败")
        require(src.namelist() == dst.namelist(), "候选文件条目发生意外增删")
        changed = [name for name in src.namelist() if src.read(name) != dst.read(name)]
        require(changed == ["ppt/slides/slide5.xml"], "候选出现预期外修改")
        require(src.read("ppt/vbaProject.bin") == dst.read("ppt/vbaProject.bin"), "VBA 字节发生变化")
        unchanged_count = len(src.namelist()) - len(changed)

    report = {
        "original_sha256": SOURCE_SHA256, "candidate_sha256": sha256(candidate_bytes),
        "changed_entries": changed, "unchanged_entries": unchanged_count,
        "vba_unchanged": True, "original_unchanged": True, "zip_crc_ok": True,
        "sounds": sound_info, "runtime_verified": False, "unresolved": UNRESOLVED,
    }
    files = {
        "quiz_template-fixed.pptm": candidate_bytes,
        "verification.json": (json.dumps(report, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
        "README.txt": (
            "StageDeck 题目模板局部修复包\n\n"
            "仅第5页 back 的形状和文字绑定改为 模块3.Click_Back，VBA 代码不变。\n"
            "sound 文件夹为模板内嵌音频的原样副本，须与 PPT 放在同一目录。\n\n"
            + UNRESOLVED + "\n\n"
            "请先在 Windows PowerPoint 单独打开此副本验收，避免与原件同时打开造成宏名称混淆。\n"
        ).encode("utf-8"),
        **{"sound/" + name: data for name, data in sounds.items()},
    }
    output.mkdir(parents=True, exist_ok=False)
    for name, data in files.items():
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as handle:
            handle.write(data)
    with ZipFile(bundle, "x", compression=ZIP_DEFLATED) as archive:
        for name, data in files.items():
            archive.writestr(output.name + "/" + name, data)
    with ZipFile(bundle) as archive:
        require(archive.testzip() is None, "交付 ZIP CRC 校验失败")
        for name, data in files.items():
            require((output / name).read_bytes() == data, "输出文件校验失败: " + name)
            require(archive.read(output.name + "/" + name) == data, "交付 ZIP 内容不一致: " + name)
    require(sha256(source.read_bytes()) == SOURCE_SHA256, "原件哈希发生变化")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    print("输出:", output)
    print("交付包:", bundle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", nargs="?", type=Path, default=ROOT / "quiz_template.pptm")
    parser.add_argument("--output", type=Path, default=ROOT / "out/quiz-template-fixed")
    args = parser.parse_args()
    try:
        prepare(args.source, args.output)
    except (OSError, ValueError) as error:
        parser.exit(1, f"停止: {error}\n")


if __name__ == "__main__":
    main()
