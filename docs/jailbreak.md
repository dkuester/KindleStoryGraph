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

## Launcher: KUAL Next

Classic KUAL (the Java "booklet") stopped working on firmware ~5.19.3+, because
Amazon removed the Java API it relied on. On modern firmware (5.19.4+,
including 5.20.1) use **KUAL Next**, a native launcher that reads the same
`config.xml` + `menu.json` extension format — KindleStoryGraph's extension works
unchanged.

- KUAL Next: https://github.com/thiagokokada/kual-next (MobileRead thread:
  https://www.mobileread.com/forums/showthread.php?t=374955)
- Alternative (C++/GTK): KUALA — https://github.com/kasparcode/kuala

Install KUAL Next (or KUALA) before installing KindleStoryGraph.

## Credits

- Véra jailbreak: the KindleModding community (kindlemodding.org, MobileRead).
- KUAL (legacy) by Twobob/Ixtab/Stepk/NiLuJe; KUAL Next by k0kada
  (thiagokokada); KUALA by kasparcode.
- Kindle content-catalog (`cc.db`) schema: reverse-engineered and documented by
  [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard).
