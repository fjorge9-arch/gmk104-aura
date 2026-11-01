"""Mede, pela webcam, o período com que a matiz passa por um ponto do teclado no efeito WAVE (arco-íris)
em cada nível de velocidade (0..4). Rode com o app gmk104-aura FECHADO.
Uso: python measure_keyboard_wave.py x0,y0,x1,y1 [segundos_por_nivel=12]"""
import sys, time, cv2, numpy as np, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "python"))
from gmk104aura.keyboard import GMK104, ID_EFFECT, ID_SPEED

roi = tuple(int(v) for v in sys.argv[1].split(","))
per_level = float(sys.argv[2]) if len(sys.argv) > 2 else 12.0
WAVE = int(sys.argv[3]) if len(sys.argv) > 3 else 1  # id do efeito a medir

kb = GMK104()
if not kb.connect():
    sys.exit("teclado não encontrado")
orig_effect = kb.get_value(ID_EFFECT, 1)[0]
orig_speed = kb.get_value(ID_SPEED, 1)[0]
cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
cap.set(cv2.CAP_PROP_FRAME_WIDTH, 1280); cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, 0.25); cap.set(cv2.CAP_PROP_AUTO_WB, 0)
time.sleep(1.0)
x0, y0, x1, y1 = roi

def hue_mean(hsv):
    m = (hsv[..., 1] > 80) & (hsv[..., 2] > 60)
    if m.sum() < 20:
        return None
    ang = np.deg2rad(hsv[..., 0][m].astype(np.float64) * 2.0)
    return np.rad2deg(np.arctan2(np.sin(ang).mean(), np.cos(ang).mean())) % 360

try:
    kb.set_value(ID_EFFECT, WAVE)
    for level in range(5):
        kb.set_value(ID_SPEED, level)
        time.sleep(1.5)
        t0 = time.perf_counter(); ts = []; hs = []
        while time.perf_counter() - t0 < per_level:
            ok, f = cap.read()
            if not ok:
                continue
            h = hue_mean(cv2.cvtColor(f[y0:y1, x0:x1], cv2.COLOR_BGR2HSV))
            if h is not None:
                ts.append(time.perf_counter() - t0); hs.append(h)
        if len(hs) < 20:
            print(f"nível {level}: poucos pontos"); continue
        t = np.array(ts); u = np.unwrap(np.deg2rad(hs))
        slope = np.polyfit(t, u, 1)[0]
        period = 2 * np.pi / abs(slope) if slope else float("inf")
        print(f"nível {level}: {period:.2f} s por volta da matiz, sentido {'crescente' if slope > 0 else 'decrescente'} (n={len(hs)})", flush=True)
finally:
    cap.release()
    kb.set_value(ID_EFFECT, orig_effect)
    kb.set_value(ID_SPEED, orig_speed)
    kb.close()
    print("teclado restaurado")
