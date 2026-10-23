"""Export SpeD ParakeetRo 110M (hybrid TDT+CTC) to the transcribe-rs Parakeet layout.

Run in a venv with `nemo_toolkit[asr] onnx onnxruntime onnx-asr` next to SpeD.nemo
(SpeD-ParakeetRo_110M_TDT-CTC.nemo from HF). Then package the output dir with
`tar --format=ustar --owner=0 --group=0 -cf - sped-parakeet-ro-110m-int8 | gzip -9`.

Mirrors istupakov/onnx-asr's parakeet-tdt export: model.export() on the
RNNT/TDT branch -> encoder-model.onnx + decoder_joint-model.onnx, then
int8 dynamic quantization, vocab.txt with <blk> appended, and the 80-mel NeMo
preprocessor saved under the name transcribe-rs hard-codes (nemo128.onnx).
"""
import shutil
from pathlib import Path

import nemo.collections.asr as nemo_asr
import onnx_asr
from onnxruntime.quantization import QuantType, quantize_dynamic

out = Path("sped-parakeet-ro-110m-int8")
fp32 = Path("fp32")
out.mkdir(exist_ok=True)
fp32.mkdir(exist_ok=True)

model = nemo_asr.models.ASRModel.restore_from("SpeD.nemo", map_location="cpu")
model.eval()
# Hybrid model: switch export + decoding to the TDT (RNNT) branch, not CTC.
model.change_decoding_strategy(decoder_type="rnnt")
model.set_export_config({"decoder_type": "rnnt"})
print("cur_decoder:", getattr(model, "cur_decoder", "?"))

model.export(str(fp32 / "model.onnx"))
print("exported:", sorted(p.name for p in fp32.iterdir()))

for name in ("encoder-model", "decoder_joint-model"):
    quantize_dynamic(
        str(fp32 / f"{name}.onnx"),
        str(out / f"{name}.int8.onnx"),
        weight_type=QuantType.QUInt8,
    )

tokens = [*model.tokenizer.vocab, "<blk>"]
assert len(tokens) == 1025, len(tokens)
with (out / "vocab.txt").open("w", encoding="utf-8", newline="\n") as f:
    for i, tok in enumerate(tokens):
        f.write(f"{tok} {i}\n")

pre = Path(onnx_asr.__file__).parent / "preprocessors" / "data" / "nemo80.onnx"
shutil.copy(pre, out / "nemo128.onnx")  # 80-mel; name is hard-coded by transcribe-rs

(out / "config.json").write_text(
    '{"model_type": "nemo-conformer-tdt", "features_size": 80, "subsampling_factor": 8}\n',
    encoding="utf-8",
)  # for onnx-asr; transcribe-rs ignores it
(out / "README.md").write_text(
    "SpeD ParakeetRo 110M TDT-CTC (gabrielpirlo/Sped_ParakeetRomanian_110M_TDT-CTC),\n"
    "TDT branch exported to ONNX and int8-quantized for transcribe-rs.\n"
    "nemo128.onnx is istupakov/onnx-asr's 80-mel nemo80.onnx under the name\n"
    "transcribe-rs expects. License: Apache-2.0. Paper: arXiv 2511.03361.\n",
    encoding="utf-8",
)
print("done:", sorted((p.name, p.stat().st_size) for p in out.iterdir()))
