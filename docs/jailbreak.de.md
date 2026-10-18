# Jailbreak-Voraussetzung

*[English version](jailbreak.md)*

KindleStoryGraph liest `/var/local/cc.db` (den Inhaltskatalog des nativen
Readers), der **nicht** über USB sichtbar und nur als Root zugänglich ist. Ein
gejailbreakter Kindle ist deshalb harte Voraussetzung — auf einem unmodifizierten
Gerät geht es nicht.

> Ein Jailbreak erlischt die Garantie und geschieht vollständig auf eigenes
> Risiko. Er ist zum normalen Bücherlesen nicht nötig; er ermöglicht nur diese
> Art von geräteseitigem Werkzeug.

## Jailbreak: Véra

Der aktuelle Jailbreak für moderne Geräte ist **Véra** (KT5/PW5/KT6/PW6/CS/KS/
KS2). Er ist bestätigt funktionierend auf dem Kindle Basic 2024 (KT6) mit
Firmware 5.20.1.

- Anleitung: https://kindlemodding.org/jailbreaking/Vera
- Support-Thread (MobileRead): https://www.mobileread.com/forums/showthread.php?t=374816

Falls deine genaue Firmware-Version nicht in der Anleitung gelistet ist, gilt
der übliche Rat: die nächstliegende gelistete Version probieren.

## KUAL

KUAL (Kindle Unified Application Launcher) stellt das Gerätemenü bereit, über
das KindleStoryGraph gestartet wird.

- Thread: https://www.mobileread.com/forums/showthread.php?t=203326
- Auf Firmware 5.x den Build `KUAL-KDK-2.0.azw2` verwenden.

KUAL vor KindleStoryGraph installieren.

## Credits

- Véra-Jailbreak: die KindleModding-Community (kindlemodding.org, MobileRead).
- KUAL: NiLuJe und Mitwirkende.
- Schema des Kindle-Inhaltskatalogs (`cc.db`): reverse-engineered und
  dokumentiert von
  [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard).
