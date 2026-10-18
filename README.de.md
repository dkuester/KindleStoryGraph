# KindleStoryGraph

StoryGraph-Synchronisierung für den nativen Kindle-Reader — das Kindle-Pendant
zu [NickelStoryGraph](https://github.com/dkuester/NickelStorygraph)
(Kobo/Tolino).

Synchronisiert Lesefortschritt und Lesestatus (wird gelesen / gelesen) von
einem gejailbreakten Kindle zu [The StoryGraph](https://thestorygraph.com),
ohne die Reader-App zu wechseln.

> **Status:** Phase 1–4 abgeschlossen. Reader, StoryGraph-Client, Sync-Engine,
> KUAL-Paketierung und Cross-Compile-Tooling sind fertig und der Sync ist auf dem
> Gerät verifiziert. Dazu: Opt-in-Allowlist-Sync, Mindestfortschritts-Schwelle,
> manuelle Titel-Zuordnung und ein rein manueller Modus. Siehe
> [docs/architecture.md](docs/architecture.md).

*[English version](README.md)*

## Funktionsweise

Anders als beim Kobo lässt sich der Kindle-Reader nicht prozessintern hooken.
KindleStoryGraph liest den Kindle-Inhaltskatalog (`cc.db`), ermittelt den
Lesefortschritt und überträgt ihn über dieselbe HTTP-Schicht zu StoryGraph, die
NickelStoryGraph nutzt.

Zwei Betriebsarten:

- **Manuell** — „Sync now" im KUAL-Menü führt einen einzelnen Durchlauf aus und
  beendet sich; im Hintergrund läuft nichts.
- **Daemon** — „Start sync daemon" pollt `cc.db` alle 15 Minuten und überträgt
  Änderungen automatisch.

- Nur der native Reader — kein KOReader nötig.
- Read-only: fasst weder `cc.db` noch die Bücher an.
- Sparsames Polling, Wi-Fi-abhängig, suspend-bewusst.

## Funktionen

- **Opt-in-Allowlist** (Standard): Im KUAL-Menü wählst du, welche Bücher syncen;
  `sync_scope=all` in `config.ini` synchronisiert stattdessen alle Bücher.
- **Mindestfortschritts-Schwelle**: Bücher unter 2 % (der Sprung beim ersten
  Öffnen) werden ignoriert, bis du wirklich liest.
- **Manuelle Zuordnung**: `cde_key → StoryGraph-ID` für Titel, die die Suche
  nicht zuordnen kann.
- **Diff-basierter Sync**: Nur Fortschritts-*Änderungen* werden übertragen,
  pro Buch gedrosselt — nie ein Voll-Upload der Bibliothek.

## Voraussetzungen

- Ein gejailbreakter Kindle (z. B. [Véra](https://kindlemodding.org/jailbreaking/Vera/)).
- Firmware ca. 5.17+ (entwickelt und getestet auf 5.20.1, Kindle Basic 2024).

## Installation

Die vollständige Anleitung steht in [docs/install.de.md](docs/install.de.md);
die Jailbreak-Voraussetzung in [docs/jailbreak.de.md](docs/jailbreak.de.md).

## Bauen

Das statische armhf-(musl-)Binary cross-kompilieren und die KUAL-Erweiterung
zusammenbauen (Docker nötig):

    just build && just package

Die Erweiterung entsteht in `packaging/extension/`; den Ordner nach
`/mnt/us/extensions/kindlestorygraph` auf dem Kindle kopieren. Getaggte
`v*`-Releases bauen per CI ein Zip (`.forgejo/workflows/release.yml`).

## Projektstatus

Phase 1–4 abgeschlossen, der Sync ist auf dem Gerät verifiziert. Der
vollständige Plan samt Umsetzungsphasen steht in
[docs/architecture.md](docs/architecture.md). Künftige Arbeit ist in
[ROADMAP.md](ROADMAP.md) festgehalten.

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
- [KindleModding](https://kindlemodding.org/) — Véra-Jailbreak, Hotfix, KUAL
  Next, MRPI und KPM, die Infrastruktur, auf der dieses Projekt aufsetzt.
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
