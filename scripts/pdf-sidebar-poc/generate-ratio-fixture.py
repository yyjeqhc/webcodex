"""One page matched to the measured PDF viewport, with geometric QA markers."""
import argparse
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.cidfonts import UnicodeCIDFont

parser = argparse.ArgumentParser()
parser.add_argument("--width", type=int, required=True)
parser.add_argument("--height", type=int, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
if not (200 <= args.width <= 2000 and 200 <= args.height <= 2000):
    parser.error("measured viewport dimensions must be between 200 and 2000")
args.output.parent.mkdir(parents=True, exist_ok=True)
w, h = args.width, args.height
pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
pdf = canvas.Canvas(str(args.output), pagesize=(w, h), invariant=1)
pdf.setTitle(f"Sidebar ratio calibration {w} x {h}")
pdf.setFillColorRGB(0.96, 0.98, 1)
pdf.rect(0, 0, w, h, fill=1, stroke=0)
pdf.setStrokeColorRGB(0.72, 0.8, 0.86)
pdf.setLineWidth(0.5)
for x in range(20, w, 20):
    pdf.line(x, 20, x, h - 20)
for y in range(20, h, 20):
    pdf.line(20, y, w - 20, y)
pdf.setStrokeColorRGB(0.12, 0.25, 0.36)
pdf.setLineWidth(2)
pdf.rect(10, 10, w - 20, h - 20)
pdf.setFillColorRGB(0.1, 0.22, 0.34)
pdf.setFont("Helvetica-Bold", min(24, w / 24))
pdf.drawCentredString(w / 2, h - 70, "PDF.js geometry check")
pdf.setFont("Helvetica", 12)
pdf.drawCentredString(w / 2, h - 96, f"{w} x {h} pt | ratio {w / h:.6f}")
pdf.setFont("STSong-Light", 15)
pdf.drawCentredString(w / 2, h - 124, "中文、圆形及四角标记应完整显示")
r = min(w, h) * 0.21
pdf.setFillColorRGB(0.06, 0.52, 0.59)
pdf.circle(w / 2, h / 2, r, fill=1, stroke=0)
pdf.setFillColorRGB(1, 1, 1)
pdf.setFont("Helvetica-Bold", 18)
pdf.drawCentredString(w / 2, h / 2 + 8, "CIRCLE")
pdf.setFont("Helvetica", 12)
pdf.drawCentredString(w / 2, h / 2 - 14, "Equal width and height")
for x, y, label, color in [(10, h - 38, "TL", (0.85, 0.25, 0.18)),
                          (w - 38, h - 38, "TR", (0.85, 0.55, 0.08)),
                          (10, 10, "BL", (0.25, 0.4, 0.8)),
                          (w - 38, 10, "BR", (0.35, 0.65, 0.3))]:
    pdf.setFillColorRGB(*color)
    pdf.rect(x, y, 28, 28, fill=1, stroke=0)
    pdf.setFillColorRGB(1, 1, 1)
    pdf.setFont("Helvetica-Bold", 10)
    pdf.drawCentredString(x + 14, y + 10, label)
pdf.setFillColorRGB(0.1, 0.22, 0.34)
pdf.setFont("Helvetica", 11)
pdf.drawCentredString(w / 2, 68, "Synthetic fixture - no private document")
pdf.showPage()
pdf.save()
print(f"Created {args.output.name}: {w} x {h} pt, {args.output.stat().st_size} bytes")
