"""Mede a cor de uma ou duas regiões da webcam ao longo do tempo (placa e, opcionalmente, teclado) e
registra também o instante em que o LastProfile.xml do Aura muda (= quando o efeito foi aplicado).

Uso: python measure.py SEGUNDOS saida.csv x0,y0,x1,y1 [x0,y0,x1,y1]
Saída: CSV com t (s desde o início), mtime_mudou (0/1), e por região: hue (0..360), sat, val médios
dos pixels saturados; depois um resumo com período estimado por regressão da matiz desdobrada.
"""
import os, sys, time, csv, cv2, numpy as np

PROFILE = r"C:\Program Files (x86)\LightingService\LastProfile.xml"
secs = float(sys.argv[1]); out = sys.argv[2]
rois = [tuple(int(v) for v in a.split(",")) for a in sys.argv[3:]]

cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
cap.set(cv2.CAP_PROP_FRAME_WIDTH, 1280); cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
# exposição fixa evita a câmera "corrigir" a cor; se não aceitar, segue no automático
cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, 0.25); cap.set(cv2.CAP_PROP_AUTO_WB, 0)
time.sleep(1.0)

def region_color(hsv, roi):
    x0, y0, x1, y1 = roi
    r = hsv[y0:y1, x0:x1]
    m = (r[..., 1] > 90) & (r[..., 2] > 90)
    if m.sum() < 20:
        return None
    h = r[..., 0][m].astype(np.float64) * 2.0  # OpenCV: 0..180 -> graus
    # média circular da matiz
    ang = np.deg2rad(h)
    mean = np.rad2deg(np.arctan2(np.sin(ang).mean(), np.cos(ang).mean())) % 360
    return mean, float(r[..., 1][m].mean()), float(r[..., 2][m].mean())

t0 = time.perf_counter()
mt0 = os.path.getmtime(PROFILE)
rows = []
changed_at = None
print("gravando... clique no efeito no Armoury Crate agora", flush=True)
while time.perf_counter() - t0 < secs:
    ok, f = cap.read()
    if not ok:
        continue
    t = time.perf_counter() - t0
    mt = os.path.getmtime(PROFILE)
    ch = 0
    if mt != mt0:
        mt0 = mt; ch = 1
        if changed_at is None:
            changed_at = t
    hsv = cv2.cvtColor(f, cv2.COLOR_BGR2HSV)
    row = [round(t, 3), ch]
    for roi in rois:
        c = region_color(hsv, roi)
        row += [None, None, None] if c is None else [round(c[0], 1), round(c[1]), round(c[2])]
    rows.append(row)
cap.release()

with open(out, "w", newline="") as fh:
    w = csv.writer(fh)
    w.writerow(["t", "mudou"] + [f"{k}{i}" for i in range(len(rois)) for k in ("hue", "sat", "val")])
    w.writerows(rows)
print(f"{len(rows)} quadros em {secs}s ({len(rows)/secs:.1f} fps); perfil mudou em t={changed_at}")

# resumo: período por regressão da matiz desdobrada (por região)
for i in range(len(rois)):
    pts = [(r[0], r[2 + 3 * i]) for r in rows if r[2 + 3 * i] is not None]
    if len(pts) < 20:
        print(f"região {i}: poucos pontos"); continue
    t = np.array([p[0] for p in pts]); h = np.unwrap(np.deg2rad([p[1] for p in pts]))
    # usa só o trecho depois da mudança de perfil (+1,5 s), se houver
    sel = t > ((changed_at or 0) + 1.5)
    if sel.sum() > 20:
        t, h = t[sel], h[sel]
    slope, icpt = np.polyfit(t, h, 1)  # rad/s
    period = 2 * np.pi / abs(slope) if slope else float("inf")
    direction = "matiz crescente (verde->ciano->azul)" if slope > 0 else "matiz decrescente (verde->amarelo->vermelho)"
    # instante em que a matiz passou por 120° (verde) pela primeira vez após a mudança
    hdeg = np.rad2deg(h) % 360
    first_green = next((tt for tt, hh in zip(t, hdeg) if abs(hh - 120) < 8), None)
    print(f"região {i}: período {period:.2f} s por volta, {direction}, "
          f"1º verde em t={first_green}, perfil mudou em t={changed_at}")
