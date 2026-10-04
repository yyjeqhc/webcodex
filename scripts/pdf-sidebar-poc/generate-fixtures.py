"""Synthetic, deterministic fixtures; no user documents or system fonts."""
from pathlib import Path
from io import BytesIO
from random import Random
from PIL import Image, ImageDraw
from reportlab.pdfgen import canvas
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.cidfonts import UnicodeCIDFont
from reportlab.lib.utils import ImageReader

out = Path(__file__).parent / "fixtures"
out.mkdir(exist_ok=True)

def document(name):
    return canvas.Canvas(str(out / name), pagesize=(480, 640), invariant=1)

pdf = document("text.pdf")
for page in range(1, 4):
    pdf.setFillColorRGB(0.12, 0.22, 0.35)
    pdf.setFont("Helvetica-Bold", 22)
    pdf.drawString(40, 570, "PDF.js sidebar experiment")
    pdf.setFont("Helvetica", 13)
    pdf.drawString(40, 530, "Selectable text, no external resources.")
    pdf.drawString(40, 506, f"Page {page} of 3. Search token: offline-preview.")
    pdf.setFillColorRGB(0.20, 0.55, 0.64)
    pdf.roundRect(40, 290, 400, 150, 12, fill=1, stroke=0)
    pdf.setFillColorRGB(1, 1, 1)
    pdf.setFont("Helvetica-Bold", 32)
    pdf.drawString(62, 352, f"{page:02d} / 03")
    pdf.showPage()
pdf.save()

pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
pdf = document("cjk.pdf")
pdf.setFont("STSong-Light", 22)
pdf.drawString(40, 570, "中文 PDF 侧边栏预览实验")
pdf.setFont("STSong-Light", 15)
pdf.drawString(40, 525, "汉字可见，文字可以选择和提取。")
pdf.drawString(40, 493, "简体与繁體：离线资源、字体、分页。")
pdf.setFont("Helvetica", 12)
pdf.drawString(40, 440, "Unembedded CID font: exercises bundled CMaps.")
pdf.showPage()
pdf.save()

random = Random(0)
grain = bytearray()
for _ in range(960 * 1280):
    shade = random.randrange(224, 249)
    grain.extend((shade, shade, shade))
image = Image.frombytes("RGB", (960, 1280), bytes(grain))
draw = ImageDraw.Draw(image)
draw.rectangle((70, 90, 890, 310), fill="#1f5261")
draw.text((105, 150), "SCANNED PAGE - image only", fill="white", font_size=44)
for index in range(6):
    y = 440 + index * 80
    draw.rounded_rectangle((100, y, 860 - index * 45, y + 22), 8, fill="#9da9a7")
draw.ellipse((600, 930, 810, 1140), fill="#e5a75a")
stream = BytesIO()
image.save(stream, format="JPEG", quality=90)
pdf = document("scan.pdf")
pdf.drawImage(ImageReader(stream), 0, 0, 480, 640)
pdf.showPage()
pdf.save()
print("Generated text.pdf, cjk.pdf, scan.pdf")
