# file3: plán dotiahnutia

Cieľ: terminálový nástroj **na čítanie kódu**, ktorý nahradí VS Code pri prezeraní projektu.
Hlavné požiadavky:

1. Celý strom súborov, otváranie klikaním aj klávesnicou.
2. Dobrý vzhľad v rámci možností terminálu, dôraz na detail.
3. Vyhľadanie súboru podľa názvu (ako Ctrl+P) a textu v celom projekte (ako Ctrl+Shift+F).

Mimo rozsahu: editovanie, LSP, pluginy, git operácie. Existujúce git funkcie (značka zmeneného
súboru `●`, filter `f`, diff `d`) zostávajú tak, ako sú, len sa prenesú do nového stromu.

## Fázy

- [x] **1. Kostra pre ďalšiu prácu**
  - Rozdeliť `app.rs` na stav stromu, náhľadu a vyskakovacích okien (overlay); `main.rs` iba
    rozdeľuje udalosti podľa toho, čo má fokus.
  - Zapnúť myš v termináli (crossterm `EnableMouseCapture`) a pamätať si pozície panelov z
    posledného vykreslenia, aby sa dalo zistiť, na čo sa kliklo.
  - Prekresľovať iba po udalosti, nie každých 16 ms (dnes app zbytočne vyťažuje CPU).
  - Overenie: existujúce testy prechádzajú, správanie sa nemení.

- [x] **2. Strom súborov**
  - Jeden strom od koreňa projektu (adresár, kde sa app spustí) namiesto prechádzania po
    adresároch. Priečinky sa načítajú až pri rozbalení.
  - Klávesy: `↑↓` pohyb, `→`/`Enter` rozbaliť alebo otvoriť, `←` zbaliť alebo skočiť na rodiča.
  - Myš: klik vyberie a otvorí súbor, klik na priečinok ho rozbalí alebo zbalí, koliesko roluje.
  - Rešpektovať `.gitignore` (crate `ignore`); `h` prepína skryté a ignorované súbory.
  - Odsadenie s vodiacimi čiarami, priečinky pred súbormi, značka `●` aj na priečinku, ktorý
    obsahuje zmenu.
  - Overenie: testy rozbalenia a zbalenia stromu, ručne kliknúť v Windows Terminal.

- [x] **3. Náhľad súboru**
  - Čísla riadkov, rolovanie kolieskom myši, `Home`/`End`, `Ctrl+G` = skok na riadok.
  - Prepínač zalamovania dlhých riadkov a horizontálne rolovanie, keď sa nezalamuje.
  - Zvýraznenie syntaxe vypočítať raz pri otvorení súboru a uložiť (nie pri každom vykreslení).
  - Jasná hláška pre binárny súbor, príliš veľký súbor a prázdny súbor.
  - `/` = hľadanie v otvorenom súbore, `n`/`N` ďalší/predchádzajúci výskyt, zvýraznené zhody.
  - Overenie: plynulé rolovanie na súbore s ~10 000 riadkami.

- [x] **4. Hľadanie súboru podľa názvu (Ctrl+P)**
  - Vyskakovacie okno s poľom na písanie; zoznam sa filtruje počas písania (crate `nucleo`).
  - Zvýraznené písmená, ktoré sa zhodujú; `Enter` otvorí súbor a rozbalí k nemu strom.
  - Zoznam súborov sa načítava na pozadí, UI pri veľkom projekte nezamrzne.
  - Overenie: odozva pod 50 ms na projekte s desiatkami tisíc súborov.

- [x] **5. Hľadanie textu v projekte (Ctrl+Shift+F)**
  - Knižnice z ripgrepu (`grep-searcher`, `grep-regex`, `ignore`), beží na pozadí, výsledky
    pribúdajú postupne a hľadanie sa dá prerušiť.
  - Výsledky zoskupené podľa súboru, s riadkom a zvýraznenou zhodou; náhľad vybraného
    výsledku vpravo.
  - `Enter` alebo klik otvorí súbor na danom riadku a zvýrazní zhodu.
  - Prepínače ako vo VS Code: veľké/malé písmená, celé slovo, regex.
  - Overenie: testy hľadania na dočasnom adresári, ručne na veľkom repe.

- [x] **6. Vzhľad a detaily**
  - Všetky farby na jedno miesto (jedna paleta); dnes sú rozsypané po `ui.rs`.
  - Zaoblené okraje, zreteľné označenie panelu, ktorý má fokus.
  - Ikony súborov cez Nerd Font, s prepínačom na obyčajné znaky, ak font chýba.
  - Hlavička s cestou k súboru (breadcrumb); stavový riadok s klávesami pre aktuálny kontext.
  - `?` = okno s nápovedou klávesov.
  - Šírku stromu meniť ťahaním deliacej čiary myšou.
  - Zmysluplné prázdne stavy (žiadne výsledky, prázdny priečinok).
  - Overenie: prejsť každú obrazovku pri úzkom (80 stĺpcov) aj širokom termináli.

- [x] **7. Upratanie**
  - Aktualizovať README (funkcie, klávesy, odporúčaný font).
  - Overenie: `cargo clippy` bez varovaní, `cargo test` prechádza.

## Otvorené otázky

- **Ctrl+Shift+F:** niektoré terminály ho nevedia odlíšiť od Ctrl+F. Ak nebude fungovať,
  náhradná skratka bude `Ctrl+F`.
- **Ikony:** máš nainštalovaný Nerd Font vo Windows Terminal? Ak nie, buď ho doinštalujeme,
  alebo zostaneme pri obyčajných znakoch.

## Druhé kolo

- [x] **8. Témy**
  - Farby rozhrania ako prepínateľné témy: Dark Modern (dnešná), Material Palenight, One Dark, Light.
  - Ku každej téme zladené farby kódu; Palenight ako zabudovaný `.tmTheme`.
  - Overenie: každá téma vykreslená vo virtuálnom termináli, kontrast textu a výberu.

- [x] **9. Paleta príkazov (F1) a nastavenia**
  - F1 otvorí zoznam príkazov s filtrom ako vo VS Code; téma sa pri prechádzaní šípkami mení hneď.
  - Nastavenia (téma, ikony, zalamovanie, editor) sa ukladajú do `%APPDATA%\file3\config.toml`.
  - Overenie: testy uloženia a načítania nastavení, paleta ovládaná klávesmi aj myšou.

- [x] **10. Automatická obnova**
  - Sledovanie zmien súborov: strom, otvorený súbor, značky zmien a zoznam súborov sa obnovia samé.
  - Zmeny v `.git` a v ignorovaných priečinkoch (napr. `target/` počas buildu) sa ignorujú.
  - Overenie: test, ktorý zmení súbor na disku a čaká na obnovu.

- [x] **11. Späť / dopredu a nedávne súbory**
  - `Alt+←` / `Alt+→` vráti na predchádzajúce miesto (súbor + pozícia) po skoku.
  - Prázdne Ctrl+P ukáže najprv naposledy otvorené súbory.

- [x] **12. Zapamätanie stavu projektu**
  - Rozbalené priečinky, otvorený súbor, pozícia a nedávne súbory ostanú aj po reštarte.

- [x] **13. Filter v hľadaní**
  - Polia „files to include“ a „files to exclude“ (napr. `src/**/*.ts`, `*.test.*`).

- [x] **14. Kopírovanie cesty a otvorenie v editore**
  - `y` skopíruje cestu súboru, `Y` cestu s číslom riadku (napr. `src/app.rs:120`).
  - `e` otvorí súbor na danom riadku vo VS Code (príkaz editora sa dá zmeniť v nastaveniach).

- [x] **15. Go to symbol**
  - `Ctrl+Shift+O` (alebo `Ctrl+R`): zoznam funkcií, tried a nadpisov v súbore s filtrom a skokom.

- [x] **16. Upratanie**
  - README, nápoveda `?`, `cargo clippy` bez varovaní, `cargo test` prechádza.
