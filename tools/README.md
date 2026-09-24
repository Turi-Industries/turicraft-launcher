# Outils embarqués

Les deux jars de [packwiz-installer](https://github.com/packwiz/packwiz-installer)
et de son [bootstrap](https://github.com/packwiz/packwiz-installer-bootstrap),
inclus tels quels dans l'exécutable (`src-tauri/src/packwiz.rs`). Le launcher
lance toujours le bootstrap avec `--bootstrap-no-update` : sans cela, il
interroge l'API GitHub à chaque lancement, sans authentification, et reçoit
un HTTP 403 dès que le quota de l'adresse IP est atteint.

Ils ne sont pas couverts par la licence GPL du launcher :

- packwiz-installer et packwiz-installer-bootstrap : licence MIT,
  © comp500 et les contributeurs de packwiz (texte ci-dessous) ;
- les bibliothèques qu'ils embarquent (Gson, Okio, Commons CLI, Kotlin…) :
  Apache 2.0 et MIT, détail et textes dans `META-INF/` de chaque jar.

```
MIT License

Copyright (c) comp500

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
