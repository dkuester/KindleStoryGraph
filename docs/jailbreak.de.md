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

## Launcher: KUAL Next

Das klassische KUAL (das Java-„Booklet") funktioniert ab Firmware ca. 5.19.3+
nicht mehr, weil Amazon die zugrunde liegende Java-API entfernt hat. Auf
moderner Firmware (5.19.4+, einschließlich 5.20.1) **KUAL Next** verwenden —
ein nativer Launcher, der dasselbe `config.xml`- + `menu.json`-Format liest; die
KindleStoryGraph-Erweiterung funktioniert damit unverändert.

- KUAL Next: https://github.com/thiagokokada/kual-next (MobileRead-Thread:
  https://www.mobileread.com/forums/showthread.php?t=374955)
- Alternative (C++/GTK): KUALA — https://github.com/kasparcode/kuala

KUAL Next (oder KUALA) vor KindleStoryGraph installieren.

## Credits

- Véra-Jailbreak: die KindleModding-Community (kindlemodding.org, MobileRead).
- KUAL (legacy) von Twobob/Ixtab/Stepk/NiLuJe; KUAL Next von k0kada
  (thiagokokada); KUALA von kasparcode.
- Schema des Kindle-Inhaltskatalogs (`cc.db`): reverse-engineered und
  dokumentiert von
  [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard).
