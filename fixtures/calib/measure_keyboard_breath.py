"""Mede o período da respiração nativa do GMK104 em cada nível de velocidade (0..4) pela webcam.
Comanda o teclado direto (efeito BREATHE, cor atual), grava o brilho da região do teclado, e no fim
devolve o efeito de cor única. Rode com o app gmk104-aura FECHADO (ele disputaria o teclado).
Uso: python measure_keyboard_breath.py x0,y0,x1,y1 [segundos_por_nivel=12]"""
import sys, time, cv2, numpy as np, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "python"))
from gmk104aura.keyboard import GMK104, ID_EFFECT, ID_SPEED, ID_BRIGHTNESS

roi = tuple(int(v) for v in sys.argv[1].split(","))
per_level = float(sys.argv[2]) if len(sys.argv) > 2 else 12.0
BREATHE, LIGHT = (int(sys.argv[3]) if len(sys.argv) > 3 else 5), 6  # efeito a medir

kb = GMK104()
if not kb.connect():
    sys.exit("teclado não encontrado")
orig_effect = kb.get_value(ID_EFFECT, 1)[0]
orig_speed = kb.get_value(ID_SPEED, 1)[0]
print("efeito/velocidade originais:", orig_effect, orig_speed, flush=True)

cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
cap.set(cv2.CAP_PROP_FRAME_WIDTH, 1280); cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, 0.25); cap.set(cv2.CAP_PROP_AUTO_WB, 0)
time.sleep(1.0)
x0, y0, x1, y1 = roi
results = {}
try:
    kb.set_value(ID_EFFECT, BREATHE)
    for level in range(5):
        kb.set_value(ID_SPEED, level)
        time.sleep(1.5)  # transição
        t0 = time.perf_counter(); ts = []; vs = []
        while time.perf_counter() - t0 < per_level:
            ok, f = cap.read()
            if not ok:
                continue
            ts.append(time.perf_counter() - t0)
            vs.append(float(cv2.cvtColor(f[y0:y1, x0:x1], cv2.COLOR_BGR2HSV)[..., 2].mean()))
        t = np.array(ts); v = np.array(vs) - np.mean(vs); n = len(v); fps = n / (t[-1] - t[0])
        spec = np.abs(np.fft.rfft(v * np.hanning(n))); freqs = np.fft.rfftfreq(n, d=1 / fps)
        k = int(np.argmax(spec[1:])) + 1
        cross = [t[i] for i in range(1, n) if v[i - 1] < 0 <= v[i]]
        med = float(np.median(np.diff(cross))) if len(cross) > 2 else float("nan")
        results[level] = (1 / freqs[k], med, float(v.max() - v.min()))
        print(f"nível {level}: período FFT {1/freqs[k]:.2f} s, mediana cruzamentos {med:.2f} s, amplitude {v.max()-v.min():.0f} (fps {fps:.1f})", flush=True)
finally:
    cap.release()
    kb.set_value(ID_EFFECT, orig_effect)
    kb.set_value(ID_SPEED, orig_speed)
    kb.close()
    print("teclado restaurado")
