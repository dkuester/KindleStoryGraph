# KindleStoryGraph installieren

*[English version](install.md)*

## Voraussetzungen

1. Ein **gejailbreakter** Kindle (siehe [jailbreak.de.md](jailbreak.de.md)).
   Der Lesefortschritt liegt in `/var/local/cc.db`, das nur mit Root zugänglich
   ist — der Jailbreak ist harte Voraussetzung, keine Option.
2. **KUAL Next** (oder KUALA) installiert — siehe [jailbreak.de.md](jailbreak.de.md).
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
   enthält `config.xml`, `menu.json`, `config_example.ini`,
   `allowlist.example.ini`, `manual_mappings.example.ini` und `bin/` (Binary
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
min_progress=2.0
manual_mappings_path=/mnt/us/extensions/kindlestorygraph/manual_mappings.ini
sync_scope=allowlist
allowlist_path=/mnt/us/extensions/kindlestorygraph/allowlist.ini
search_cache_path=/mnt/us/extensions/kindlestorygraph/search_cache.json
menu_path=/mnt/us/extensions/kindlestorygraph/menu.json
```

## 4. Daemon starten

KUAL Next → KindleStoryGraph → **Start sync daemon**.

Das startet den Daemon im Hintergrund; er pollt `cc.db` alle 15 Minuten und
schiebt Änderungen zu StoryGraph, sobald WLAN verfügbar ist. „Sync now" führt
sofort einen Durchlauf aus. „Stop sync daemon" beendet ihn.

## 5. Verifizieren

Der Daemon loggt nach `extensions/kindlestorygraph/kindlestorygraph.log` auf
dem Kindle. Nach einem Sync dort (und auf deinem StoryGraph-Regal) prüfen, ob
Bücher zugeordnet und Fortschritte übertragen wurden.

## 6. Auswählen, welche Bücher syncen (Allowlist)

Standardmäßig (`sync_scope=allowlist`) werden nur die Bücher synchronisiert,
die du explizit auswählst — nichts landet auf StoryGraph, solange du nicht
zustimmst. Genau so hältst du private oder „spicy" Titel von deinem Regal fern.

Auswahl im KUAL-Menü:

1. KUAL Next → KindleStoryGraph → **Sync selection** öffnen.
2. Jedes Buch mit Lesefortschritt wird gelistet; antippen zum Umschalten. Der
   Zusatz `[sync]` markiert die Bücher, die synchronisiert werden — KUAls
   eigenes Häkchen erscheint nur, solange eine Taste gedrückt ist, deshalb
   steckt der Zustand im Namen. Die Liste wird nach jedem Tipp neu geschrieben,
   das Menü bleibt dabei offen.
3. Falls ein frisch angefangenes Buch noch fehlt, zuerst **Refresh book list**
   antippen.

Das Menü springt nach jedem Tipp auf seine oberste Ebene zurück (KUAL Next lädt
das komplette Menü neu, um den neuen Zustand zu übernehmen) — bei einer großen
Bibliothek kostet das pro Buch einen zusätzlichen Tipp.

Die Auswahl liegt in `extensions/kindlestorygraph/allowlist.ini` (ein
`cde_key` pro Zeile); du kannst die Datei stattdessen auch per USB bearbeiten.

Um stattdessen alle Bücher zu synchronisieren, in `config.ini` setzen:

```ini
sync_scope=all
```

Dann werden alle Bücher mit Fortschritt synchronisiert und die Auswahl wird
ignoriert.

Fortschritt läuft auf StoryGraph nie rückwärts: Vor jedem Push wird der
aktuelle StoryGraph-Wert gelesen, und der Push wird übersprungen
(`skipped_behind` im Log), wenn das Buch dort schon als gelesen markiert ist
oder auf einem anderen Gerät weiter gelesen wurde. Ein Buch, dessen Sync
fehlschlägt (Netzwerk, abgelaufene Cookies), steht als `failed` im Log und wird
beim nächsten Durchlauf erneut versucht; die übrigen Bücher werden trotzdem
synchronisiert.

## 7. Bücher mit StoryGraph verknüpfen

Bücher, deren Titel sich nicht automatisch zuordnen lässt, werden im Log als
`no_match` übersprungen und bleiben unverknüpft. Du kannst sie — oder ein Buch
neu mit einem anderen StoryGraph-Buch verknüpfen — auf zwei Arten zuordnen.

### 7a. Aus dem KUAL-Menü wählen (empfohlen)

1. In KUAL Next → KindleStoryGraph **Find matches for books** antippen. Das
   sucht StoryGraph (Titel + Autor) für jedes *unverknüpfte* Buch ab und
   speichert die Top-5-Treffer zwischen. Dafür braucht es WLAN und gültige
   Cookies; außerdem dauert es (eine Suche pro Buch), deshalb schließt sich
   KUAL währenddessen — wenn der Kindle wieder auf dem Startbildschirm ist,
   KUAL erneut öffnen, dann ist **Link books** da.
2. **Link books** öffnen. Jedes unverknüpfte Buch ist gelistet; antippen, um
   die passenden StoryGraph-Bücher zu sehen (Titel — Autor), dann das
   richtige antippen. Das Menü bleibt offen und das Buch verschwindet aus
   **Link books**.
3. Die Auswahl wird in `manual_mappings.ini` geschrieben und greift beim
   nächsten Sync; das Buch bekommt danach in **Sync selection** den Marker
   `[linked]`.

Bereits verknüpfte Bücher (durch einen früheren Sync oder eine manuelle
Zuordnung) erscheinen nicht in **Link books**; ihr Eintrag in **Sync
selection** trägt den `[linked]`-Marker, sodass du auf einen Blick siehst, was
verknüpft ist und was nicht.

### 7b. `manual_mappings.ini` von Hand bearbeiten

Um ein Buch manuell zu verknüpfen oder eine falsche automatische Zuordnung zu
überschreiben:

1. Den `cde_key` aus dem Log notieren (`NoMatch { cde_key: "..." }`) oder mit
   `kindlestorygraph snapshot <cc.db>` ermitteln.
2. Das Buch auf https://app.thestorygraph.com öffnen und die UUID aus der URL
   kopieren (`app.thestorygraph.com/books/<UUID>`), oder `search "<Titel>"`
   ausführen.
3. Eine Zeile in `extensions/kindlestorygraph/manual_mappings.ini` eintragen
   (von `manual_mappings.example.ini` kopieren):

   ```
   <cde_key>=<storygraph_book_id>
   ```

Die Zuordnung greift beim nächsten Sync-Zyklus und überschreibt jede
automatische Zuordnung.

## Hinweise & Einschränkungen

- Der Daemon wird manuell über KUAL gestartet. Nach einem Neustart startet er
  **noch nicht** automatisch; erneut über KUAL starten (Upstart/Hotfix-Autostart
  ist geplant).
- Fortschritt wird vom Kindle zu StoryGraph übertragen (bei Konflikten gewinnt
  der Kindle).
- Der erste Sync überträgt den *aktuellen* Gerätefortschritt aller erlaubten
  Bücher ab `min_progress` (Standard 2 %) — nur kurz geöffnete Bücher (der
  Kindle springt auf ~1 %) werden ignoriert. Das Titel-Matching ist gegen
  StoryGraph verifiziert, aber `no_match`-Bücher werden übersprungen (siehe
  Log) und können [verknüpft werden](#7-bücher-mit-storygraph-verknüpfen).
