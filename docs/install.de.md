# KindleStoryGraph installieren

*[English version](install.md)*

## Voraussetzungen

1. Ein **gejailbreakter** Kindle (siehe [jailbreak.de.md](jailbreak.de.md)).
   Der Lesefortschritt liegt in `/var/local/cc.db`, das nur mit Root zugänglich
   ist — der Jailbreak ist harte Voraussetzung, keine Option.
2. **KUAL** (Kindle Unified Application Launcher) installiert.
3. WLAN-Verbindung auf dem Kindle für den Sync.

## 1. StoryGraph-Session-Cookies besorgen

StoryGraph hat keine öffentliche API; KindleStoryGraph spricht dieselben
Web-App-Endpunkte wie dein eingeloggter Browser. Du brauchst zwei Cookies:

1. In einem Desktop-Browser bei https://app.thestorygraph.com einloggen.
2. DevTools öffnen → Application/Storage → Cookies → `app.thestorygraph.com`.
3. Den Wert des Cookies, dessen Name mit `_storygraph_session` beginnt,
   kopieren → das ist dein `session_cookie`.
4. Den Wert des Cookies `remember_user_token` kopieren → das ist dein
   `remember_user_token`.

Behandle diese wie Passwörter — sie gewähren vollen Zugriff auf dein Konto.

## 2. Erweiterung auf den Kindle kopieren

1. Kindle per USB anschließen und einhängen.
2. Den Ordner `kindlestorygraph` nach `/mnt/us/extensions/kindlestorygraph`
   kopieren (also `extensions/kindlestorygraph/` auf dem USB-Laufwerk). Er
   enthält `config.xml`, `menu.json`, `config_example.ini` und `bin/` (Binary
   + Startskripte).
3. Kindle sicher auswerfen.

## 3. Konfigurieren

Entweder vor dem Kopieren (`config_example.ini` bearbeiten → in `config.ini`
umbenennen) oder nach dem ersten „Start"-Tipp (das Startskript kopiert beim
ersten Lauf `config_example.ini` nach `config.ini`). Mindestens eintragen:

```ini
session_cookie=<Wert aus Schritt 1>
remember_user_token=<Wert aus Schritt 1>
```

Die übrigen Defaults passen fürs Gerät:

```ini
cc_db_path=/var/local/cc.db
state_path=/mnt/us/extensions/kindlestorygraph/state.json
finish_threshold=99.5
progress_step=1.0
poll_interval=900
match_threshold=0.6
```

## 4. Daemon starten

KUAL → KindleStoryGraph → **Start sync daemon**.

Das startet den Daemon im Hintergrund; er pollt `cc.db` alle 15 Minuten und
schiebt Änderungen zu StoryGraph, sobald WLAN verfügbar ist. „Sync now" führt
sofort einen Durchlauf aus. „Stop sync daemon" beendet ihn.

## 5. Verifizieren

Der Daemon loggt nach `extensions/kindlestorygraph/kindlestorygraph.log` auf
dem Kindle. Nach einem Sync dort (und auf deinem StoryGraph-Regal) prüfen, ob
Bücher zugeordnet und Fortschritte übertragen wurden.

## Hinweise & Einschränkungen

- Der Daemon wird manuell über KUAL gestartet. Nach einem Neustart startet er
  **noch nicht** automatisch; erneut über KUAL starten (Upstart/Hotfix-Autostart
  ist geplant).
- Fortschritt wird vom Kindle zu StoryGraph übertragen (bei Konflikten gewinnt
  der Kindle).
- Der erste Sync überträgt den *aktuellen* Gerätefortschritt aller Bücher mit
  `percent > 0`; das Titel-Matching ist gegen StoryGraph verifiziert, aber
  `no_match`-Bücher werden übersprungen (siehe Log) und können später manuell
  zugeordnet werden.
