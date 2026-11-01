"""Temperatura da CPU pelo mesmo caminho que o Armoury Crate usa: COM `aaHM.acpiHmData2`
(servido pelo "ASUS Com Service", asComSvc).

Sensor "CPU" (id 0x06030012 = 100859922), valor em décimos de grau Celsius. Exige processo
elevado (administrador): sem elevação o CoCreateInstance devolve E_ACCESSDENIED e este módulo
apenas informa que a temperatura está indisponível.
"""
from __future__ import annotations

import logging

log = logging.getLogger("gmk104.temp")

SENSOR_ID_CPU = 0x06030012
SENSOR_NAME_CPU = "CPU"


class CpuTemp:
    def __init__(self) -> None:
        self._hm = None
        self._sensors = None
        self._idx: int | None = None
        self.available = False
        self.last: float | None = None
        self.error: str | None = None

    def connect(self) -> bool:
        try:
            import comtypes
            import comtypes.client

            comtypes.CoInitialize()
            self._hm = comtypes.client.CreateObject("aaHM.acpiHmData2")
            self._hm.SetIsMonitoring(1)
            self._hm.Refresh()
            self._sensors = self._hm.Sensors
            self._idx = None
            for i in range(self._sensors.Count):
                it = self._sensors.Item(i)
                if it.id == SENSOR_ID_CPU or it.name == SENSOR_NAME_CPU:
                    self._idx = i
                    break
            if self._idx is None:
                self.error = "sensor CPU não encontrado"
                return False
            self.available, self.error = True, None
            log.info("sensor de temperatura da CPU pronto (índice %d)", self._idx)
            return True
        except Exception as e:  # E_ACCESSDENIED sem admin, serviço parado, etc.
            self.error = str(e)
            self.available = False
            return False

    def read(self) -> float | None:
        if not self.available and not self.connect():
            return None
        try:
            self._hm.Refresh()
            raw = int(self._sensors.Item(self._idx).current)
            if raw > 0xFFFF:  # sensor ausente devolve 0xFFFF0000 etc.
                return self.last
            self.last = raw / 10.0
            return self.last
        except Exception as e:
            log.warning("falha ao ler temperatura (%s); vou reconectar ao serviço", e)
            self.available = False
            return None
