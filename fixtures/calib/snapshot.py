"""Tira uma foto da webcam (para escolher a região do LED) e marca em verde as áreas saturadas/brilhantes."""
import sys, time, cv2, numpy as np

out = sys.argv[1] if len(sys.argv) > 1 else "cam.jpg"
cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
cap.set(cv2.CAP_PROP_FRAME_WIDTH, 1280); cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
time.sleep(1.5)  # exposição estabilizar
for _ in range(10):
    ok, f = cap.read()
cap.release()
if not ok:
    sys.exit("sem quadro")
hsv = cv2.cvtColor(f, cv2.COLOR_BGR2HSV)
mask = (hsv[..., 1] > 100) & (hsv[..., 2] > 120)
marked = f.copy()
marked[mask] = (0.5 * marked[mask] + np.array([0, 255, 0]) * 0.5).astype(np.uint8)
h, w = f.shape[:2]
for x in range(0, w, 100):
    cv2.line(marked, (x, 0), (x, h), (255, 255, 255), 1); cv2.putText(marked, str(x), (x + 2, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.4, (255, 255, 255), 1)
for y in range(0, h, 100):
    cv2.line(marked, (0, y), (w, y), (255, 255, 255), 1); cv2.putText(marked, str(y), (2, y + 14), cv2.FONT_HERSHEY_SIMPLEX, 0.4, (255, 255, 255), 1)
cv2.imwrite(out, f)
cv2.imwrite(out.replace(".jpg", "_marcado.jpg"), marked)
print("ok", f.shape, "saturados:", int(mask.sum()))
