"""Levanta, pela webcam, como cada efeito nativo do GMK104 se comporta: variação de matiz no tempo,
variação espacial de matiz (várias cores ao mesmo tempo) e variação de brilho. Rode com o app FECHADO.
Uso: python survey_keyboard_effects.py x0,y0,x1,y1 [segundos_por_efeito=6]"""
import sys, time, cv2, numpy as np, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "python"))
from gmk104aura.keyboard import GMK104, ID_EFFECT, ID_SPEED, ID_COLOR

NAMES = ["OFF", "WAVE", "COLOUR_CLOUD", "VORTEX", "MIX_COLOUR", "BREATHE", "LIGHT", "SLOWLY_OFF", "STONE", "LASER",
         "STARRY", "FLOWERS_OPEN", "TRAVERSE", "WAVE_BAR", "METEOR", "RAIN", "SCAN", "TRIGGER_COLOUR", "CENTER_SPREAD"]
roi = tuple(int(v) for v in sys.argv[1].split(","))
per = float(sys.argv[2]) if len(sys.argv) > 2 else 6.0
x0, y0, x1, y1 = roi

kb = GMK104()
if not kb.connect():
    sys.exit("teclado não encontrado")
orig_effect = kb.get_value(ID_EFFECT, 1)[0]; orig_speed = kb.get_value(ID_SPEED, 1)[0]
cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
cap.set(cv2.CAP_PROP_FRAME_WIDTH, 1280); cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, 0.25); cap.set(cv2.CAP_PROP_AUTO_WB, 0)
time.sleep(1.0)

def circ_mean_deg(h):
    a = np.deg2rad(h); return np.rad2deg(np.arctan2(np.sin(a).mean(), np.cos(a).mean())) % 360
def circ_std_deg(h):
    a = np.deg2rad(h); r = np.hypot(np.sin(a).mean(), np.cos(a).mean()); return np.rad2deg(np.sqrt(-2 * np.log(max(r, 1e-9))))

try:
    kb.set_value(ID_SPEED, 2)
    for eff in [1, 2, 3, 4, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18]:
        kb.set_value(ID_EFFECT, eff)
        time.sleep(1.0)
        t0 = time.perf_counter(); means = []; spatial = []; vals = []
        while time.perf_counter() - t0 < per:
            ok, f = cap.read()
            if not ok:
                continue
            hsv = cv2.cvtColor(f[y0:y1, x0:x1], cv2.COLOR_BGR2HSV)
            m = (hsv[..., 1] > 80) & (hsv[..., 2] > 60)
            vals.append(float(hsv[..., 2].mean()))
            if m.sum() > 20:
                h = hsv[..., 0][m].astype(np.float64) * 2.0
                means.append(circ_mean_deg(h)); spatial.append(circ_std_deg(h))
        if len(means) < 5:
            print(f"{eff:2d} {NAMES[eff]:14s}: quase sem pixels coloridos (brilho médio {np.mean(vals):.0f})", flush=True); continue
        print(f"{eff:2d} {NAMES[eff]:14s}: matiz no tempo ±{circ_std_deg(np.array(means)):5.1f}°  matiz no espaço ±{np.mean(spatial):5.1f}°  "
              f"brilho ±{np.std(vals):5.1f} (média {np.mean(vals):.0f})", flush=True)
finally:
    cap.release()
    kb.set_value(ID_EFFECT, orig_effect); kb.set_value(ID_SPEED, orig_speed); kb.close()
    print("teclado restaurado")
