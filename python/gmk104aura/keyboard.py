"""Controle do ZUOYA GMK104 pelo canal VIA Raw HID (interface usage page 0xFF60, usage 0x61).

Protocolo (QMK/VIA "custom values", canal 3 = rgb_matrix), relatórios de 32 bytes:
  [0x07, canal, id, dados...]  id_custom_set_value
  [0x08, canal, id]            id_custom_get_value (resposta ecoa o valor)
  id 1 = brilho (0..4; o firmware limita a 4 e valores maiores apagam o teclado)
  id 2 = efeito (6 = LIGHT_MODE, cor única estática)
  id 3 = velocidade (0..4)
  id 4 = cor [hue 0..255, sat 0..255]
"""
from __future__ import annotations

import logging

import hid

log = logging.getLogger("gmk104.keyboard")

VENDOR_ID = 0x320F
PRODUCT_IDS = (0x5055, 0x5088)  # cabo USB, receptor 2.4G
VIA_USAGE_PAGE, VIA_USAGE = 0xFF60, 0x61

CMD_SET, CMD_GET = 0x07, 0x08
CH_RGB_MATRIX = 3
ID_BRIGHTNESS, ID_EFFECT, ID_SPEED, ID_COLOR = 1, 2, 3, 4
EFFECT_STATIC = 6
BRIGHTNESS_MAX = 4


def find_path() -> bytes | None:
    for pid in PRODUCT_IDS:
        for d in hid.enumerate(VENDOR_ID, pid):
            if d["usage_page"] == VIA_USAGE_PAGE and d["usage"] == VIA_USAGE:
                return d["path"]
    return None


class GMK104:
    def __init__(self) -> None:
        self._dev = None
        self.last_color: tuple[int, int] | None = None

    @property
    def connected(self) -> bool:
        return self._dev is not None

    def connect(self) -> bool:
        path = find_path()
        if not path:
            return False
        try:
            dev = hid.device()
            dev.open_path(path)
            self._dev = dev
            log.info("teclado conectado: %s", path.decode(errors="replace"))
            return True
        except OSError as e:
            log.warning("falha ao abrir teclado: %s", e)
            self._dev = None
            return False

    def close(self) -> None:
        if self._dev:
            try:
                self._dev.close()
            except Exception:
                pass
        self._dev = None

    def _xfer(self, *payload: int) -> bytes | None:
        if not self._dev:
            raise OSError("teclado desconectado")
        buf = bytearray(33)  # byte 0 = report id 0
        buf[1:1 + len(payload)] = bytes(payload)
        self._dev.write(bytes(buf))
        r = self._dev.read(32, timeout_ms=300)
        return bytes(r) if r else None

    def set_value(self, value_id: int, *data: int) -> bool:
        r = self._xfer(CMD_SET, CH_RGB_MATRIX, value_id, *data)
        return r is not None and r[0] == CMD_SET

    def get_value(self, value_id: int, n: int = 2) -> bytes | None:
        r = self._xfer(CMD_GET, CH_RGB_MATRIX, value_id)
        return r[3:3 + n] if r else None

    def set_color_hs(self, hue: int, sat: int) -> bool:
        ok = self.set_value(ID_COLOR, hue & 0xFF, sat & 0xFF)
        if ok:
            self.last_color = (hue & 0xFF, sat & 0xFF)
        return ok

    def set_static_effect(self) -> bool:
        return self.set_value(ID_EFFECT, EFFECT_STATIC)

    def set_brightness(self, level: int) -> bool:
        return self.set_value(ID_BRIGHTNESS, max(0, min(BRIGHTNESS_MAX, level)))

    def apply(self, hue: int, sat: int, ensure_effect: bool = True) -> bool:
        """Aplica a cor (e o efeito estático, se preciso). Reconecta se necessário.

        Retorna False se o teclado não respondeu; o chamador tenta de novo no próximo ciclo.
        """
        try:
            if not self._dev and not self.connect():
                return False
            if ensure_effect:
                cur = self.get_value(ID_EFFECT, 1)
                if cur is None or cur[0] != EFFECT_STATIC:
                    self.set_static_effect()
            return self.set_color_hs(hue, sat)
        except OSError as e:
            log.warning("erro de comunicação com o teclado (%s); vou reconectar", e)
            self.close()
            return False
