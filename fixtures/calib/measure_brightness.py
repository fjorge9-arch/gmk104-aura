"""Mede o brilho (canal V) de regiões da webcam ao longo do tempo e estima o período da respiração
(Intermitente) por autocorrelação. Uso: python measure_brightness.py SEGUNDOS saida.csv x0,y0,x1,y1 [...]"""
import os, sys, time, csv, cv2, numpy as np

PROFILE = r"C:\Program Files (x86)\LightingService\LastProfile.xml"
SCRIPT = r"C:\Program Files (x86)\LightingService\script\LastScript.xml"
def _mt():
    return max(os.path.getmtime(PROFILE), os.path.getmtime(SCRIPT))
secs = float(sys.argv[1]); out = sys.argv[2]
rois = [tuple(int(v) for v in a.split(",")) for a in sys.argv[3:]]

cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
cap.set(cv2.CAP_PROP_FRAME_WIDTH, 1280); cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, 0.25); cap.set(cv2.CAP_PROP_AUTO_WB, 0)
time.sleep(1.0)
t0 = time.perf_counter(); mt0 = _mt(); changed_at = None; rows = []
print("gravando...", flush=True)
while time.perf_counter() - t0 < secs:
    ok, f = cap.read()
    if not ok:
        continue
    t = time.perf_counter() - t0
    mt = _mt()
    if mt != mt0:
        mt0 = mt
        if changed_at is None:
            changed_at = t
    hsv = cv2.cvtColor(f, cv2.COLOR_BGR2HSV)
    row = [round(t, 3)]
    for (x0, y0, x1, y1) in rois:
        row.append(round(float(hsv[y0:y1, x0:x1, 2].mean()), 1))
    rows.append(row)
cap.release()
with open(out, "w", newline="") as fh:
    w = csv.writer(fh); w.writerow(["t"] + [f"val{i}" for i in range(len(rois))]); w.writerows(rows)
print(f"{len(rows)} quadros ({len(rows)/secs:.1f} fps); perfil mudou em t={changed_at}")

for i in range(len(rois)):
    t = np.array([r[0] for r in rows]); v = np.array([r[1 + i] for r in rows])
    sel = t > ((changed_at or 0) + 2.0)
    t, v = t[sel], v[sel]
    if len(v) < 50:
        print(f"região {i}: poucos pontos"); continue
    fps = len(t) / (t[-1] - t[0])
    v = v - v.mean()
    ac = np.correlate(v, v, mode="full")[len(v) - 1:]
    ac /= ac[0]
    # primeiro máximo local após o primeiro vale
    lag = 1
    while lag < len(ac) - 1 and ac[lag] <= ac[lag + 1] or ac[lag] > 0:
        lag += 1
        if lag >= len(ac) - 1: break
    peak = lag + int(np.argmax(ac[lag:min(len(ac), lag + int(fps * 10))]))
    period = peak / fps
    print(f"região {i}: período da respiração ~ {period:.2f} s (fps {fps:.1f}); brilho min/max = {v.min()+0:.0f}/{v.max():.0f} relativo à média")
