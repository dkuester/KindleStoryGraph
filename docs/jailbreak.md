# Jailbreak prerequisite

*[Deutsche Übersetzung](jailbreak.de.md)*

KindleStoryGraph reads `/var/local/cc.db` (the native reader's content catalog),
which is **not** visible over USB and is only accessible as root. A jailbroken
Kindle is therefore a hard prerequisite — there is no way to do this on a stock
device.

> Jailbreaking voids your warranty and is done entirely at your own risk. It is
> not required to read books normally; it only enables this kind of device-side
> tooling.

## Jailbreak: Véra

The current jailbreak for modern devices is **Véra** (KT5/PW5/KT6/PW6/CS/KS/
KS2). It is confirmed working on the Kindle Basic 2024 (KT6) with firmware
5.20.1.

- Guide: https://kindlemodding.org/jailbreaking/Vera
- Support thread (MobileRead): https://www.mobileread.com/forums/showthread.php?t=374816

If your exact firmware version is not listed in the guide, the standard advice
is to try the next-closest version listed.

## KUAL

KUAL (Kindle Unified Application Launcher) provides the on-device menu that
launches KindleStoryGraph.

- Thread: https://www.mobileread.com/forums/showthread.php?t=203326
- On firmware 5.x, use the `KUAL-KDK-2.0.azw2` build.

Install KUAL before installing KindleStoryGraph.

## Credits

- Véra jailbreak: the KindleModding community (kindlemodding.org, MobileRead).
- KUAL: NiLuJe and contributors.
- Kindle content-catalog (`cc.db`) schema: reverse-engineered and documented by
  [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard).
