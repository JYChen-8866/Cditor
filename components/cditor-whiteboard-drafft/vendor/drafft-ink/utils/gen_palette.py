import colorsys

colors = {
    "Red":     [(253,252,252),(254,242,242),(254,226,226),(254,202,202),(252,165,165),(248,113,113),(239,68,68),(220,38,38),(185,28,28),(153,27,27),(127,29,29),(69,10,10)],
    "Orange":  [(253,252,250),(255,247,237),(255,237,213),(254,215,170),(253,186,116),(251,146,60),(249,115,22),(234,88,12),(194,65,12),(154,52,18),(124,45,18),(67,20,7)],
    "Amber":   [(254,254,254),(255,251,235),(254,243,199),(253,230,138),(252,211,77),(251,191,36),(245,158,11),(217,119,6),(180,83,9),(146,64,14),(120,53,15),(69,26,3)],
    "Yellow":  [(253,253,253),(254,252,232),(254,249,195),(254,240,138),(253,224,71),(250,204,21),(234,179,8),(202,138,4),(161,98,7),(133,77,14),(113,63,18),(66,32,6)],
    "Lime":    [(253,255,248),(247,254,231),(236,252,203),(217,249,157),(190,242,100),(163,230,53),(132,204,22),(101,163,13),(77,124,15),(63,98,18),(54,83,20),(26,46,5)],
    "Green":   [(252,252,252),(240,253,244),(220,252,231),(187,247,208),(134,239,172),(74,222,128),(34,197,94),(22,163,74),(21,128,61),(22,101,52),(20,83,45),(5,46,22)],
    "Emerald": [(254,254,254),(236,253,245),(209,250,229),(167,243,208),(110,231,183),(52,211,153),(16,185,129),(5,150,105),(4,120,87),(6,95,70),(6,78,59),(2,44,34)],
    "Teal":    [(254,254,254),(240,253,250),(204,251,241),(153,246,228),(94,234,212),(45,212,191),(20,184,166),(13,148,136),(15,118,110),(17,94,89),(19,78,74),(4,47,46)],
    "Cyan":    [(254,254,254),(236,254,255),(207,250,254),(165,243,252),(103,232,249),(34,211,238),(6,182,212),(8,145,178),(14,116,144),(21,94,117),(22,78,99),(8,51,68)],
    "Sky":     [(249,253,255),(240,249,255),(224,242,254),(186,230,253),(125,211,252),(56,189,248),(14,165,233),(2,132,199),(3,105,161),(7,89,133),(12,74,110),(8,47,73)],
    "Blue":    [(253,254,255),(239,246,255),(219,234,254),(191,219,254),(147,197,253),(96,165,250),(59,130,246),(37,99,235),(29,78,216),(30,64,175),(30,58,138),(23,37,84)],
    "Indigo":  [(246,248,254),(238,242,255),(224,231,255),(199,210,254),(165,180,252),(129,140,248),(99,102,241),(79,70,229),(67,56,202),(55,48,163),(49,46,129),(30,27,75)],
    "Violet":  [(250,249,255),(245,243,255),(237,233,254),(221,214,254),(196,181,253),(167,139,250),(139,92,246),(124,58,237),(109,40,217),(91,33,182),(76,29,149),(46,16,101)],
    "Purple":  [(254,254,255),(250,245,255),(243,232,255),(233,213,255),(216,180,254),(192,132,252),(168,85,247),(147,51,234),(126,34,206),(107,33,168),(88,28,135),(59,7,100)],
    "Fuchsia": [(254,251,255),(253,244,255),(250,232,255),(245,208,254),(240,171,252),(232,121,249),(217,70,239),(192,38,211),(162,28,175),(134,25,143),(112,26,117),(74,4,78)],
    "Pink":    [(253,248,251),(253,242,248),(252,231,243),(251,207,232),(249,168,212),(244,114,182),(236,72,153),(219,39,119),(190,24,93),(157,23,77),(131,24,67),(80,7,36)],
    "Rose":    [(254,249,250),(255,241,242),(255,228,230),(254,205,211),(253,164,175),(251,113,133),(244,63,94),(225,29,72),(190,18,60),(159,18,57),(136,19,55),(76,5,25)],
    "Slate":   [(250,251,252),(248,250,252),(241,245,249),(226,232,240),(203,213,225),(148,163,184),(100,116,139),(71,85,105),(51,65,85),(30,41,59),(15,23,42),(2,6,23)],
    "Gray":    [(251,252,253),(249,250,251),(243,244,246),(229,231,235),(209,213,219),(156,163,175),(107,114,128),(75,85,99),(55,65,81),(31,41,55),(17,24,39),(3,7,18)],
    "Zinc":    [(250,250,250),(250,250,250),(244,244,245),(228,228,231),(212,212,216),(161,161,170),(113,113,122),(82,82,91),(63,63,70),(39,39,42),(24,24,27),(9,9,11)],
    "Neutral": [(250,250,250),(250,250,250),(245,245,245),(229,229,229),(212,212,212),(163,163,163),(115,115,115),(82,82,82),(64,64,64),(38,38,38),(23,23,23),(10,10,10)],
    "Stone":   [(250,250,249),(250,250,249),(245,245,244),(231,229,228),(214,211,209),(168,162,158),(120,113,108),(87,83,78),(68,64,60),(41,37,36),(28,25,23),(12,10,9)],
}

shade_labels = ["25","50","100","200","300","400","500","600","700","800","900","950"]

def estimate_shade_15(shade_25, shade_50):
    """Use HSL: keep hue/sat from shade_25, push lightness higher (toward shade_50->25 trend)."""
    h25, l25, s25 = colorsys.rgb_to_hls(shade_25[0]/255, shade_25[1]/255, shade_25[2]/255)
    h50, l50, s50 = colorsys.rgb_to_hls(shade_50[0]/255, shade_50[1]/255, shade_50[2]/255)
    # Extrapolate lightness one step beyond 25, keep hue, boost saturation slightly
    l15 = min(0.995, l25 + (l25 - l50))
    # Keep the hue from shade_25, but ensure saturation stays perceptible
    s15 = max(s25, s50 * 0.8)  # don't let it wash out
    h15 = h25 if s25 > 0.01 else h50
    r, g, b = colorsys.hls_to_rgb(h15, l15, s15)
    return (min(255, max(0, round(r*255))), min(255, max(0, round(g*255))), min(255, max(0, round(b*255))))

shade15 = {name: estimate_shade_15(s[0], s[1]) for name, s in colors.items()}

# Print comparison
print(f"{'Name':<10} {'15 (R,G,B)':<20} {'25 (R,G,B)':<20} {'50 (R,G,B)':<20}")
for name in colors:
    s15, s25, s50 = shade15[name], colors[name][0], colors[name][1]
    print(f"{name:<10} ({s15[0]:>3},{s15[1]:>3},{s15[2]:>3})      ({s25[0]:>3},{s25[1]:>3},{s25[2]:>3})      ({s50[0]:>3},{s50[1]:>3},{s50[2]:>3})")

def rgb_hex(r,g,b): return f"#{r:02x}{g:02x}{b:02x}"
def text_color(r,g,b): return "#000" if 0.299*r+0.587*g+0.114*b > 128 else "#fff"

all_labels = ["15"] + shade_labels

html = """<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Tailwind Palette + Shade 15</title>
<style>
body{font-family:system-ui,sans-serif;background:#f5f5f5;padding:24px}
h1{font-size:20px;margin-bottom:4px}
.sub{color:#666;font-size:13px;margin-bottom:20px}
table{border-collapse:collapse;font-size:11px}
th{padding:4px 2px;font-weight:600;font-size:10px;color:#666;min-width:52px}
.swatch{width:52px;height:32px;display:flex;align-items:center;justify-content:center;font-size:9px;font-family:monospace;border:1px solid rgba(0,0,0,.04)}
.swatch:hover{outline:2px solid #333;outline-offset:-2px;z-index:1;position:relative}
.name{font-weight:600;font-size:11px;padding-right:8px;min-width:60px;text-align:right;vertical-align:middle}
.new{position:relative}
.new::after{content:"★";position:absolute;top:1px;right:2px;font-size:7px;opacity:.4}
pre{background:#fff;padding:12px;border:1px solid #ddd;font-size:12px;overflow-x:auto}
</style></head><body>
<h1>Tailwind Color Palette</h1>
<p class="sub">Shade 15 estimated via HSL extrapolation (perceptually brighter, hue-preserving). ★ = new.</p>
<table><tr><th></th>"""
for l in all_labels: html += f"<th>{l}</th>"
html += "</tr>\n"

for name, shades in colors.items():
    html += f'<tr><td class="name">{name}</td>'
    s = shade15[name]
    bg, tc = rgb_hex(*s), text_color(*s)
    html += f'<td><div class="swatch new" style="background:{bg};color:{tc}" title="{name} 15: rgb({s[0]},{s[1]},{s[2]})">{bg}</div></td>'
    for i, s in enumerate(shades):
        bg, tc = rgb_hex(*s), text_color(*s)
        html += f'<td><div class="swatch" style="background:{bg};color:{tc}" title="{name} {shade_labels[i]}: rgb({s[0]},{s[1]},{s[2]})">{bg}</div></td>'
    html += "</tr>\n"

html += """</table><br>
<h2 style="font-size:16px">Shade 15 — Rust Tuples</h2><pre>"""
for name in colors:
    s = shade15[name]
    html += f"// {name} 15\n({s[0]:>3}, {s[1]:>3}, {s[2]:>3}),\n"
html += "</pre></body></html>"

with open("/local/home/patwie/git/github.com/patwie/drafft-ink/tailwind_palette.html","w") as f:
    f.write(html)
print("\n✓ Written to tailwind_palette.html")
