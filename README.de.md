# KindleStoryGraph

StoryGraph-Synchronisierung für den nativen Kindle-Reader — das Kindle-Pendant
zu [NickelStoryGraph](https://github.com/dkuester/NickelStorygraph)
(Kobo/Tolino).

Synchronisiert Lesefortschritt und Lesestatus (wird gelesen / gelesen) von
einem gejailbreakten Kindle zu [The StoryGraph](https://thestorygraph.com),
ohne die Reader-App zu wechseln.

> **Status:** frühe Entwicklung — Phase 1 und 2 abgeschlossen. Der `cc.db`-Reader
> und der StoryGraph-Web-API-Client (Login, Suche, Status, Fortschritt) sind
> implementiert und unit-getestet; als Nächstes folgt der On-Device-Daemon
> (Phase 3). Siehe [docs/architecture.md](docs/architecture.md).

*[English version](README.md)*

## Funktionsweise

Anders als beim Kobo lässt sich der Kindle-Reader nicht prozessintern hooken.
KindleStoryGraph läuft als kleiner Daemon, der den Kindle-Inhaltskatalog
(`cc.db`) liest, das aktuelle Buch samt Prozentstand ermittelt und diesen über
dieselbe HTTP-Schicht zu StoryGraph überträgt, die NickelStoryGraph nutzt.

- Nur der native Reader — kein KOReader nötig.
- Read-only: fasst weder `cc.db` noch die Bücher an.
- Sparsames Polling, Wi-Fi-abhängig, suspend-bewusst.

## Voraussetzungen

- Ein gejailbreakter Kindle (z. B. [Véra](https://kindlemodding.org/jailbreaking/Vera/)).
- Firmware ca. 5.17+ (entwickelt und getestet auf 5.20.1, Kindle Basic 2024).

## Projektstatus

Phase 0. Der vollständige Plan samt Umsetzungsphasen steht in
[docs/architecture.md](docs/architecture.md).

## Danksagung / Credits

Dieses Projekt baut auf vorangegangener Reverse-Engineering- und
Community-Arbeit auf:

- [NickelStoryGraph](https://github.com/VictoriaKerr131/NickelStorygraph) von
  VictoriaKerr131 (Fork: [dkuester/NickelStorygraph](https://github.com/dkuester/NickelStorygraph)) —
  das Kobo/Tolino-Plugin, dessen StoryGraph-Login, Buch-Matching und
  Sync-Logik hier übernommen werden.
- [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard) —
  hat das `cc.db`-Schema (`p_percentFinished`, `p_readState`, …) und die moderne
  Lesestatus-Ablage reverse-engineered und dokumentiert, die dieses Projekt liest.
- [kaikozlov/kindle.koplugin](https://github.com/kaikozlov/kindle.koplugin) —
  Präzedenzfall für das Lesen des Kindle-Inhaltskatalogs aus On-Device-Code.
- [KindleModding](https://kindlemodding.org/) — Véra-Jailbreak, Hotfix, KUAL,
  MRPI und KPM, die Infrastruktur, auf der dieses Projekt aufsetzt.
- Das [KindleModding-Wiki](https://kindlemodding.org/kindle-hacking/) und die
  [MobileRead-Foren](https://www.mobileread.com/forums/) — Dokumentation der
  Kindle-Interna.
- [KRDS](https://www.mobileread.com/forums/showthread.php?t=322172) von j.p.s /
  tomsem — Parser für die `.sdr`-Sidecar-Dateien (referenziert; genutzt, falls
  feinere Positionsdaten benötigt werden).
- [burneracc0112/storygraph.koplugin](https://github.com/burneracc0112/storygraph.koplugin) —
  Referenz für StoryGraph-Sync-Muster (KOReader).
- [sighery.com — Transferring books and progress between Kindles](https://sighery.com/posts/transferring-content-between-kindles/) —
  Dokumentation von `cc.db` / `Entries`.

## Offenlegung: KI-Unterstützung

Dieses Projekt wird mit KI-Unterstützung entwickelt. Das Reverse Engineering
der Kindle-Lesefortschritts-Ablage und die Umsetzung entstehen in
Zusammenarbeit mit KI-Werkzeugen unter menschlicher Kontrolle. Die zugrunde
liegenden Formate und Protokolle wurden durch die oben genannte
Community-Forschung entdeckt, nicht durch KI.

## Lizenz

[MIT](LICENSE) © 2026 Daniel Küster
