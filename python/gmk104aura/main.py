"""Laço principal: lê o perfil do Aura e a temperatura da CPU, calcula a cor e envia ao GMK104.

Taxa do laço: 20 Hz (verifica perfil, teclado e tempo). A temperatura é consultada a cada
`--temp-interval` segundos (o LightingService usa 3 s). O teclado só recebe comando quando a cor
muda ou a cada `--refresh` segundos (para recuperar de troca de efeito por Fn ou reconexão).
"""
from __future__ import annotations

import argparse
import logging
import logging.handlers
import os
import sys
import threading
import time

from . import __version__
from .aura_log import AuraLogWatcher
from .aura_profile import AuraWatcher
from .cpu_temp import CpuTemp
from .keyboard import GMK104

LOG_DIR = os.path.join(os.environ.get("LOCALAPPDATA", os.path.expanduser("~")), "gmk104-aura")


def setup_logging(verbose: bool) -> None:
    os.makedirs(LOG_DIR, exist_ok=True)
    fmt = logging.Formatter("%(asctime)s %(levelname)s %(name)s: %(message)s")
    root = logging.getLogger()
    root.setLevel(logging.DEBUG if verbose else logging.INFO)
    fh = logging.handlers.RotatingFileHandler(os.path.join(LOG_DIR, "gmk104-aura.log"),
                                              maxBytes=1_000_000, backupCount=2, encoding="utf-8")
    fh.setFormatter(fmt)
    root.addHandler(fh)
    logging.getLogger("comtypes").setLevel(logging.WARNING)
    if sys.stdout and sys.stdout.isatty():
        sh = logging.StreamHandler(sys.stdout)
        sh.setFormatter(fmt)
        root.addHandler(sh)


class TempSampler(threading.Thread):
    """Lê a temperatura em segundo plano: cada leitura do serviço ASUS leva ~3 s e não pode travar o laço."""

    def __init__(self, interval: float) -> None:
        super().__init__(name="cpu-temp", daemon=True)
        self.interval = interval
        self.temp = CpuTemp()
        self.value: float | None = None
        self.error: str | None = None
        self._stop = threading.Event()

    def run(self) -> None:
        log = logging.getLogger("gmk104.temp")
        while not self._stop.is_set():
            t = self.temp.read()
            if t is not None:
                self.value, self.error = t, None
                wait = self.interval
            else:
                self.error = self.temp.error
                if self.error:
                    log.info("temperatura indisponível (%s); nova tentativa em 30 s", self.error)
                wait = 30.0
            self._stop.wait(wait)

    def stop(self) -> None:
        self._stop.set()


def is_admin() -> bool:
    try:
        import ctypes
        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except Exception:
        return False


def run(args: argparse.Namespace) -> int:
    log = logging.getLogger("gmk104")
    admin = is_admin()
    log.info("gmk104-aura %s iniciando (admin=%s)", __version__, admin)
    aura, kb, auralog = AuraWatcher(), GMK104(), AuraLogWatcher()
    sampler = TempSampler(args.temp_interval)
    if not admin:
        log.info("sem privilégios de administrador: no modo Smart a cor vem só do log do LightingService")

    period = 1.0 / args.fps
    last_sent: tuple[int, int] | None = None
    last_sent_at = 0.0
    last_band: str | None = None
    warned_no_source = False
    t_end = time.time() + args.duration if args.duration else None

    try:
        while True:
            now = time.time()
            if t_end and now >= t_end:
                log.info("duração encerrada")
                return 0
            aura.poll()
            st = aura.state

            if st.smart:
                auralog.poll()
                # 1ª fonte: cor que o LightingService registrou no log (sem admin, é a decisão exata do Aura)
                if auralog.color is not None:
                    color = auralog.color.to_hs255()
                else:
                    # 2ª fonte: temperatura da CPU via serviço ASUS (só funciona elevado)
                    cur_temp: float | None = None
                    if admin:
                        if not sampler.is_alive():
                            sampler.start()
                        cur_temp = sampler.value
                        if cur_temp is not None:
                            band = st.band(cur_temp)
                            if band != last_band:
                                log.info("CPU %.1f °C -> faixa %s (limiares %.0f/%.0f)",
                                         cur_temp, band, st.thr_low, st.thr_high)
                                last_band = band
                    if cur_temp is None:
                        # Sem fonte não há como saber a faixa: assume a faixa fria (verde no perfil
                        # padrão), que é o estado normal, em vez da cor base do perfil (vermelho).
                        if not warned_no_source:
                            log.warning("modo Smart sem fonte ainda (log do LightingService %s, "
                                        "temperatura indisponível); assumindo a faixa Thermal_Low",
                                        "sem cor" if auralog.available else "ausente em " + auralog.path)
                            warned_no_source = True
                        color = st.smart_colors.get("Thermal_Low", st.static).to_hs255()
                    else:
                        color = st.color_for_temp(cur_temp).to_hs255()
            else:
                color = st.static.to_hs255()
            due = (color != last_sent) or (now - last_sent_at >= args.refresh)
            if due:
                if kb.apply(*color):
                    if color != last_sent:
                        log.info("teclado <- hue=%d sat=%d", *color)
                    last_sent, last_sent_at = color, now
                else:
                    log.debug("teclado indisponível; tentando de novo")
                    last_sent_at = now - args.refresh + 1.0  # tenta de novo em ~1 s
            time.sleep(period)
    finally:
        sampler.stop()
        kb.close()


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(prog="gmk104-aura", description=__doc__)
    p.add_argument("--fps", type=float, default=20, help="frequência do laço (padrão 20)")
    p.add_argument("--temp-interval", type=float, default=3.0,
                   help="segundos entre leituras de temperatura (cada leitura leva ~3 s)")
    p.add_argument("--refresh", type=float, default=5.0, help="reenvio periódico da cor ao teclado (s)")
    p.add_argument("--duration", type=float, default=0, help="encerra após N segundos (0 = para sempre)")
    p.add_argument("-v", "--verbose", action="store_true")
    args = p.parse_args(argv)
    setup_logging(args.verbose)
    try:
        return run(args)
    except KeyboardInterrupt:
        return 0
    except Exception:
        logging.getLogger("gmk104").exception("erro fatal")
        return 1


if __name__ == "__main__":
    sys.exit(main())
