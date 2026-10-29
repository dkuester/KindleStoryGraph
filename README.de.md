<div align="center">

# KindleStoryGraph

**Lesefortschritt aus dem Kindle-eigenen Reader mit The StoryGraph synchronisieren. Ohne KOReader.**

[![Release](https://img.shields.io/github/v/release/dkuester/KindleStoryGraph?label=release)](https://github.com/dkuester/KindleStoryGraph/releases/latest)
[![CI](https://github.com/dkuester/KindleStoryGraph/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/dkuester/KindleStoryGraph/actions/workflows/ci.yml)
[![Kindle](https://img.shields.io/badge/Kindle-jailbroken%2C%20FW%205.17%2B-informational)](#voraussetzungen)
[![Lizenz](https://img.shields.io/github/license/dkuester/KindleStoryGraph)](LICENSE)

[Installation](docs/install.de.md) · [Funktionen](#funktionen) · [Funktionsweise](#funktionsweise) · [Bauen](#bauen) · [Architektur](docs/architecture.md) · [Releases](https://github.com/dkuester/KindleStoryGraph/releases) · [English](README.md)

</div>

KindleStoryGraph überträgt Lesefortschritt und Lesestatus (lese ich gerade /
gelesen) von einem gejailbreakten Kindle zu [The StoryGraph](https://thestorygraph.com),
während du weiter im nativen Kindle-Reader liest. Es ist das Gegenstück zu
[NickelStorygraph](https://github.com/dkuester/NickelStorygraph) für Kobo und
tolino und nutzt dieselbe Logik für StoryGraph-Login, Buchzuordnung und Sync.

> [!NOTE]
> **Stand:** Reader, StoryGraph-Client, Sync-Engine, KUAL-Paket und
> Cross-Compile-Werkzeuge sind fertig, der Sync ist auf dem Gerät geprüft
> (Kindle Basic 2024, Firmware 5.20.1). Weitere Pläne stehen in [ROADMAP.md](ROADMAP.md).

---

## Funktionen

| Funktion | Beschreibung |
|---|---|
| **Nativer Reader** | Liest den Fortschritt aus dem Inhaltskatalog des Kindle (`cc.db`). Kein KOReader, keine andere Lese-App. |
| **Manuell oder automatisch** | „Sync now“ im KUAL-Menü synchronisiert einmal. „Start sync daemon“ prüft alle 15 Minuten und überträgt Änderungen. |
| **Bücher selbst auswählen** | Standardmäßig per Allowlist: Die zu synchronisierenden Bücher wählst du im KUAL-Menü. `sync_scope=all` synchronisiert stattdessen alle. |
| **Kein versehentlicher Start** | Bücher unter 2 % Fortschritt (der Sprung beim ersten Öffnen) werden ignoriert, bis du wirklich zu lesen beginnst. |
| **Manuelle Zuordnung** | Ein Kindle-Buch einer StoryGraph-Ausgabe zuordnen, im KUAL-Menü („Find matches“ → „Link books“) oder in `manual_mappings.ini`. Verknüpfte Bücher tragen `[linked]`. |
| **Von StoryGraph entfernen** | Ein schon synchronisiertes Buch soll doch privat bleiben? „Remove from StoryGraph...“ im KUAL-Menü öffnet die Einstellungs-Oberfläche mit der Liste der synchronisierten Bücher; nach einer Bestätigung setzt sie es (oder `unsync <cde_key>`) auf StoryGraph auf „to-read“ zurück und synchronisiert es nicht mehr; „Remove completely“ löscht dort zusätzlich Leseverlauf und Rezension. |
| **Schonend fürs Gerät** | Nur *Änderungen* am Fortschritt werden übertragen, gedrosselt pro Buch. Seltene Abfragen, nur mit WLAN, Standby wird respektiert. |
| **Nur lesend** | Schreibt nie in `cc.db` oder in deine Bücher. |

## Voraussetzungen

| | |
|---|---|
| Gerät | Ein **gejailbreakter** Kindle, z. B. per [Véra](https://kindlemodding.org/jailbreaking/Vera/) ([Anleitung](docs/jailbreak.de.md)) |
| Firmware | etwa 5.17 oder neuer (entwickelt und getestet mit 5.20.1, Kindle Basic 2024) |
| Launcher | KUAL Next oder KUALA |
| Konto | Ein StoryGraph-Konto und WLAN auf dem Kindle |

## Installation

> [!IMPORTANT]
> StoryGraph hat keine öffentliche API. KindleStoryGraph meldet sich mit zwei
> Session-Cookies aus deinem Browser an. Behandle sie wie Passwörter.

1. `kindlestorygraph-<version>-kual.zip` aus dem [neuesten Release](https://github.com/dkuester/KindleStoryGraph/releases/latest) herunterladen.
2. Den Kindle per USB verbinden, den Ordner `extensions/kindlestorygraph/`
   anlegen und den Inhalt der ZIP dorthin entpacken.
3. `config_example.ini` nach `config.ini` kopieren und `session_cookie` sowie
   `remember_user_token` eintragen.
4. In KUAL: **KindleStoryGraph → Sync now** oder **Start sync daemon**.

Die vollständige Anleitung, auch zum Auslesen der Cookies und zu allen
Einstellungen, steht in [docs/install.de.md](docs/install.de.md).

## Funktionsweise

Anders als bei Kobo lässt sich der Kindle-Reader nicht im laufenden Prozess
hooken. KindleStoryGraph ist ein kleines statisches Programm, das den
Inhaltskatalog des Kindle (`cc.db`) liest, Fortschritt und Lesestatus
ausliest, jedes Buch einer StoryGraph-Ausgabe zuordnet und Änderungen über
dieselbe HTTP-Schicht wie NickelStorygraph überträgt. Details stehen
(englisch) in [docs/architecture.md](docs/architecture.md).

## Bauen

Das statische armhf-Programm (musl) cross-kompilieren und die KUAL-Erweiterung
zusammenstellen (Docker und [just](https://just.systems) nötig):

```bash
git clone https://github.com/dkuester/KindleStoryGraph.git
cd KindleStoryGraph
just build && just package
```

Die Erweiterung entsteht in `packaging/extension/`. Für die Entwicklung auf
dem Rechner: `just lint` (fmt, clippy und die Tests). `just emu-test` führt das
armhf-Programm und die KUAL-Skripte in einem [virtuellen Kindle](docs/emulator.md) aus.

Die CI führt fmt, clippy und die Tests bei jedem Push auf `main` und jedem
Pull-Request aus. Jeder Tag `v*` baut die KUAL-ZIP in der CI und hängt sie an
das Release.

## Support

| | |
|---|---|
| Fehlermeldungen und Fragen | [Issues](https://github.com/dkuester/KindleStoryGraph/issues). Bitte Kindle-Modell, Firmware-Version und den passenden Ausschnitt aus `kindlestorygraph.log` angeben. **Cookies vorher aus Logs und Konfiguration entfernen.** |
| Downloads | [Releases](https://github.com/dkuester/KindleStoryGraph/releases) |
| Geplante Funktionen | [ROADMAP.md](ROADMAP.md) |
| Frühere Historie | Pull-Requests bis v0.1.0 liegen im [archivierten Codeberg-Repo](https://codeberg.org/hortizept/KindleStoryGraph). |
| Mitwirken | Pull-Requests gegen `main`; die CI muss grün sein. |

> [!WARNING]
> Ein Jailbreak kann die Garantie kosten. KindleStoryGraph steht in keiner
> Verbindung zu Amazon oder The StoryGraph. Nutzung auf eigene Gefahr.

## Danksagung

Dieses Projekt baut auf Reverse Engineering und Arbeit aus der Community auf:

- [NickelStorygraph](https://github.com/VictoriaKerr131/NickelStorygraph) von
  VictoriaKerr131 (Fork: [dkuester/NickelStorygraph](https://github.com/dkuester/NickelStorygraph)):
  das Kobo/tolino-Plugin, dessen StoryGraph-Login, Buchzuordnung und
  Sync-Logik hier portiert sind.
- [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard):
  hat das Schema von `cc.db` (`p_percentFinished`, `p_readState`, …) und die
  neuere Speicherung des Lesestatus erschlossen und dokumentiert.
- [kaikozlov/kindle.koplugin](https://github.com/kaikozlov/kindle.koplugin):
  Vorbild für das Lesen des Inhaltskatalogs aus Code auf dem Gerät.
- [KindleModding](https://kindlemodding.org/): Véra-Jailbreak, Hotfix, KUAL
  Next, MRPI und KPM, die Infrastruktur, auf der dieses Projekt aufsetzt.
- Das [KindleModding-Wiki](https://kindlemodding.org/kindle-hacking/) und die
  [MobileRead-Foren](https://www.mobileread.com/forums/): Dokumentation der
  Kindle-Interna.
- [KRDS](https://www.mobileread.com/forums/showthread.php?t=322172) von j.p.s /
  tomsem: Parser für die `.sdr`-Begleitdateien (referenziert; genutzt, falls
  feinere Positionsdaten nötig werden).
- [burneracc0112/storygraph.koplugin](https://github.com/burneracc0112/storygraph.koplugin):
  Referenz für StoryGraph-Sync-Muster (KOReader).
- [sighery.com: Transferring books and progress between Kindles](https://sighery.com/posts/transferring-content-between-kindles/):
  Dokumentation zu `cc.db` / `Entries`.

## KI-Unterstützung

Dieses Projekt wird mit KI-Unterstützung entwickelt. Das Reverse Engineering
der Speicherung des Lesefortschritts und die Implementierung entstehen in
Zusammenarbeit mit KI-Werkzeugen unter menschlicher Kontrolle. Die
zugrundeliegenden Formate und Protokolle hat die oben genannte
Community-Forschung entdeckt, nicht die KI.

## Lizenz

[MIT](LICENSE) © 2026 Daniel Küster
