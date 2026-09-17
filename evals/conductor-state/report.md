# conductor-state kit — report

Generated 2026-09-17T09:52Z · seeds 106 (hinge 14) · synthetic test 1126

| runner | split | n | condition | decision | risk | all three | accepted | acc on accepted (cond) | mean prob (cond) | ms/case |
|---|---|---|---|---|---|---|---|---|---|---|
| jevlike-hf | seed | 92 | 55% | 13% | 30% | 2% | 100% | 55% | 71% | 164 |
| jevlike-hf | seed · hinge | 14 | 0% | 0% | 36% | 0% | 100% | 0% | 39% | 159 |
| jevlike-hf | synthetic-test | 1125 | 69% | 19% | 78% | 9% | 100% | 69% | 69% | 159 |
| jevlike-hf | seed · all | 106 | 48% | 11% | 31% | 2% | 100% | 48% | 67% | 163 |
| jevlike-tiny | seed | 92 | 92% | 97% | 43% | 37% | 100% | 92% | 97% | 6 |
| jevlike-tiny | seed · hinge | 14 | 14% | 29% | 43% | 0% | 100% | 14% | 98% | 1 |
| jevlike-tiny | synthetic-test | 1125 | 99% | 86% | 86% | 74% | 100% | 99% | 98% | 2 |
| jevlike-tiny | seed · all | 106 | 82% | 88% | 43% | 32% | 100% | 82% | 97% | 6 |
| rlcd-line | seed | 26 | 92% | 0% | 12% | 0% | 100% | 92% | 94% | 5989 |
| rlcd-line | seed · hinge | 14 | 0% | 14% | 36% | 0% | 100% | 0% | 88% | 6324 |
| rlcd-line | synthetic-test | 20 | 40% | 10% | 0% | 0% | 100% | 40% | 79% | 5875 |
| rlcd-line | seed · all | 40 | 60% | 5% | 20% | 0% | 100% | 60% | 92% | 6106 |
| rlcd-tail | seed | 26 | 69% | 0% | 38% | 0% | 100% | 69% | 81% | 9471 |
| rlcd-tail | seed · hinge | 14 | 29% | 0% | 43% | 0% | 100% | 29% | 57% | 9350 |
| rlcd-tail | seed · all | 40 | 55% | 0% | 40% | 0% | 100% | 55% | 73% | 9428 |
| vv | seed | 92 | 99% | 99% | 100% | 98% | 100% | 99% | 97% | 30 |
| vv | seed · hinge | 14 | 21% | 21% | 50% | 0% | 100% | 21% | 98% | 30 |
| vv | synthetic-test | 1125 | 100% | 99% | 98% | 97% | 100% | 100% | 99% | 30 |
| vv | seed · all | 106 | 89% | 89% | 93% | 85% | 100% | 89% | 97% | 30 |

## Seed confusion (condition) — rows: hand, columns: predicted

### jevlike-hf

- cancelled → cancelled ×2
- dead → failing ×2
- dead → progressing ×1
- dead → stalled ×8
- exhausted → exhausted ×2
- failing → looping ×2
- misrouted → misrouted ×1
- progressing → failing ×15
- progressing → looping ×10
- progressing → stalled ×15
- settled → settled ×44
- wrapping-up → failing ×1
- wrapping-up → stalled ×1
- wrapping-up → wrapping-up ×2

### jevlike-tiny

- cancelled → cancelled ×2
- dead → dead ×10
- dead → progressing ×1
- exhausted → exhausted ×2
- failing → looping ×2
- misrouted → progressing ×1
- progressing → looping ×7
- progressing → misrouted ×3
- progressing → progressing ×27
- progressing → stalled ×3
- settled → settled ×44
- wrapping-up → progressing ×2
- wrapping-up → wrapping-up ×2

### rlcd-line

- cancelled → cancelled ×2
- dead → stalled ×1
- failing → looping ×2
- misrouted → misrouted ×1
- progressing → failing ×1
- progressing → looping ×7
- progressing → stalled ×3
- settled → settled ×21
- wrapping-up → stalled ×2

### rlcd-tail

- cancelled → dead ×1
- cancelled → settled ×1
- dead → stalled ×1
- failing → stalled ×2
- misrouted → stalled ×1
- progressing → blocked ×1
- progressing → failing ×1
- progressing → progressing ×5
- progressing → settled ×2
- progressing → stalled ×2
- settled → dead ×1
- settled → settled ×17
- settled → stalled ×3
- wrapping-up → progressing ×1
- wrapping-up → settled ×1

### vv

- cancelled → cancelled ×2
- dead → dead ×11
- exhausted → exhausted ×2
- failing → looping ×2
- misrouted → misrouted ×1
- progressing → looping ×7
- progressing → progressing ×33
- settled → blocked ×1
- settled → settled ×43
- wrapping-up → progressing ×2
- wrapping-up → wrapping-up ×2

## Seed misses, by runner (hand ≠ predicted; hinge rows marked ★)

### jevlike-hf

- `1789234755480#10` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says login isn t `
- `1789234755480#103` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says the screensh`
- `1789234755480#15` hand cancelled/collect · got cancelled/resume · `$a turn cancelled last said only quiet no errors no repeats budget fresh silence unknown alive says pong no lo`
- `1789234755480#228` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says yes it s a rea`
- `1789234755480#232` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says not much on `
- `1789234755480#242` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says right and th`
- `1789234755480#258` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says here s the t`
- `1789234755480#49` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says fable isn t `
- `1789234755480#5` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ng that s no`
- `1789234755480#70` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says nothing to a`
- `1789234755480#89` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says diagnosed th`
- `1789234755480#95` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says same two let`
- `1789234755480#99` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong still i`
- `1789369589483#156` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says here s the h`
- `1789369589483#195` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says cargo s warm`
- `1789369589483#250` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says filed and ve`
- `1789369589483#265` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says done it s in`
- `1789369589483#280` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors some repeats budget fresh silence unknown alive says sent threa`
- `1789369589483#312` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading some errors no repeats budget fresh silence unknown alive says corrected `
- `1789369589483#4` hand cancelled/collect · got cancelled/resume · `$a turn cancelled just started quiet no errors no repeats budget fresh silence unknown alive`
- `1789369589483#403` hand progressing/wait · got stalled/resume · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says guard passes `
- `1789369589483#492` hand settled/collect · got settled/resume · `$a turn settled last send failed reading some errors no repeats budget fresh silence unknown alive says local `
- `1789369589483#493` hand progressing/wait · got stalled/resume · `$a turn open steer arrived quiet no errors no repeats budget fresh silence unknown alive`
- `1789369589483#494` hand misrouted/cancel · got misrouted/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says this one look`
- `1789369589483#89` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says key s fine u`
- `1789369589483#95` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong alive s`
- `1789369589483#99` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says dropped a ca`
- `1789369933818#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ok`
- `1789370009078#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ok`
- `1789370021128#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ok`
- `1789370022016#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ok`
- `1789370054476#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says teal`
- `1789370101078#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ok`
- `1789442770365#8` hand settled/collect · got settled/resume · `$a turn settled last grep ok reading no errors no repeats budget fresh silence unknown alive says mock provide`
- ★ `1789442770546#1` hand dead/escalate · got stalled/resume · `$a turn open just started quiet no errors no repeats budget fresh silence unknown gone`
- `1789442770564#4` hand settled/collect · got settled/resume · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong`
- `1789442789574#6` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says three most r`
- `1789452798155#120` hand progressing/wait · got looping/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says i ll start by`
- `1789452798155#239` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says now let me pi`
- `1789452798155#247` hand dead/resume · got failing/resume · `$a turn open last bash ok reading some errors no repeats budget fresh silence unknown gone says now let me pin`
- ★ `1789453311563#200` hand progressing/wait · got stalled/resume · `$a turn open last bg ok reading no errors looping budget fresh silence unknown alive says all green final cons`
- ★ `1789453311563#201` hand progressing/wait · got looping/resume · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says all green final `
- `1789453311563#233` hand settled/collect · got settled/resume · `$a turn settled last bash failed writing some errors no repeats budget fresh silence unknown alive says the re`
- `1789453311563#99` hand progressing/wait · got failing/resume · `$a turn open bash in flight mixed no errors no repeats budget fresh silence unknown alive says the new file is`
- `1789454784108#119` hand progressing/wait · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says cover files r`
- `1789454784108#161` hand settled/collect · got settled/resume · `$a turn settled last peers ok mixed some errors no repeats budget fresh silence unknown alive says done report`
- `1789454852561#116` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says now let me lo`
- `1789454852561#168` hand dead/resume · got stalled/resume · `$a turn open last bash ok reading no errors no repeats budget fresh silence unknown gone says now let me check`
- ★ `1789454852561#23` hand progressing/wait · got looping/resume · `$a turn open read in flight reading many errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789454852561#24` hand progressing/wait · got looping/resume · `$a turn open last read ok reading some errors looping budget fresh silence unknown alive says i ll start by or`
- `1789457216470#86` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says done only read`
- `1789459105685#109` hand progressing/wait · got failing/resume · `$a turn open edit in flight mixed some errors no repeats budget fresh silence unknown alive says now fix the i`
- `1789459105685#208` hand progressing/wait · got looping/resume · `$a turn open bash in flight reading no errors some repeats budget fresh silence unknown alive says now fix the`
- `1789459105685#244` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says done all steps`
- `1789459156365#75` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading some errors no repeats budget fresh silence unknown alive says diagnosis `
- `1789459156365#96` hand settled/collect · got settled/resume · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says not fixed th`
- `1789459450149#109` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says now let me lo`
- `1789459450149#188` hand dead/resume · got stalled/resume · `$a turn open last read ok writing no errors no repeats budget fresh silence unknown gone says now the client m`
- `1789483331584#176` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says nix eval succ`
- ★ `1789483331584#256` hand progressing/nudge · got failing/resume · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now let me verify`
- `1789483331584#258` hand wrapping-up/wait · got wrapping-up/resume · `$a turn open nudge arrived mixed no errors no repeats budget nudged silence unknown alive says now let me veri`
- `1789483331584#94` hand progressing/wait · got failing/resume · `$a turn open edit in flight writing no errors no repeats budget fresh silence unknown alive says now the mode `
- `1789483405670#27` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed some errors some repeats budget fresh silence unknown alive says test compl`
- `1789483405698#36` hand settled/collect · got settled/resume · `$a turn settled last edit ok mixed no errors no repeats budget fresh silence unknown alive says done report wr`
- `1789514384656#119` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says now let me re`
- `1789514384656#224` hand progressing/wait · got stalled/resume · `$a turn open edit in flight mixed no errors no repeats budget fresh silence unknown alive says now the cli ref`
- ★ `1789514384656#317` hand progressing/nudge · got failing/resume · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now the register `
- `1789514384656#319` hand wrapping-up/wait · got wrapping-up/resume · `$a turn open nudge arrived mixed no errors no repeats budget nudged silence unknown alive says now the registe`
- `1789543918520#29` hand dead/resume · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown gone says i ll start by `
- `1789544041771#37` hand dead/resume · got failing/resume · `$a turn open last bash ok reading no errors no repeats budget fresh silence unknown gone says baseline drvpath`
- `1789544315813#26` hand dead/resume · got progressing/resume · `$a turn open last read ok reading no errors no repeats budget fresh silence unknown gone says i ll start by re`
- `1789544606370#142` hand dead/resume · got stalled/resume · `$a turn open last bash ok reading no errors no repeats budget fresh silence unknown gone says now let me deter`
- `1789544606370#98` hand progressing/wait · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says let me test w`
- `1789545992902#25` hand dead/resume · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown gone says i ll start by `
- `1789546613822#115` hand progressing/wait · got stalled/resume · `$a turn open bash in flight mixed no errors no repeats budget fresh silence unknown alive says the top level a`
- `1789546613822#223` hand progressing/wait · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says the drvpath d`
- `1789546613822#237` hand dead/resume · got stalled/resume · `$a turn open last bash ok reading no errors no repeats budget fresh silence unknown gone says let me run contr`
- ★ `1789546613822#39` hand progressing/wait · got looping/resume · `$a turn open bash in flight reading some errors looping budget fresh silence unknown alive says i ll start by `
- `1789603005561#106` hand progressing/wait · got looping/resume · `$a turn open write in flight writing no errors some repeats budget fresh silence unknown alive says the mechan`
- `1789603005561#190` hand progressing/wait · got stalled/resume · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says i have enough`
- `1789603005561#233` hand dead/resume · got stalled/resume · `$a turn open last bash ok reading no errors no repeats budget fresh silence unknown gone says qmllint is absen`
- ★ `1789603005561#91` hand progressing/wait · got looping/resume · `$a turn open last bash ok writing no errors looping budget fresh silence unknown alive says the probe shows ro`
- `1789606499704#182` hand progressing/wait · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says let me get an`
- `1789606499704#207` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says done report wr`
- `1789606499704#92` hand progressing/wait · got stalled/resume · `$a turn open bash in flight reading no errors some repeats budget fresh silence unknown alive says now the con`
- `1789610971430#169` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says done report wr`
- `1789610971430#88` hand progressing/wait · got failing/resume · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says now update th`
- ★ `1789626749164#110` hand failing/steer · got looping/resume · `$a turn open last fetch ok reading no errors looping budget fresh silence unknown alive says i ll start by ori`
- ★ `1789626749164#114` hand failing/steer · got looping/resume · `$a turn open fetch in flight reading no errors looping budget fresh silence unknown alive says i ll start by o`
- ★ `1789626749164#226` hand progressing/wait · got looping/resume · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says let me test the `
- ★ `1789626749164#313` hand wrapping-up/wait · got stalled/resume · `$a turn open steer arrived reading no errors no repeats budget low silence unknown alive says i have the key e`
- ★ `1789626749164#314` hand wrapping-up/wait · got failing/resume · `$a turn open write in flight mixed no errors no repeats budget low silence unknown alive says i have the key e`
- `1789626749164#318` hand settled/collect · got settled/resume · `$a turn settled last write ok mixed no errors no repeats budget low silence unknown alive says report written `
- `1789626749164#324` hand settled/collect · got settled/resume · `$a turn settled last write ok mixed no errors no repeats budget fresh silence unknown alive says report writte`
- `1789630556602#114` hand progressing/wait · got stalled/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says item done now`
- `1789630556602#183` hand settled/collect · got settled/resume · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says report complet`
- `1789633647483#109` hand progressing/wait · got stalled/resume · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says now the appen`
- `1789633647483#195` hand progressing/wait · got failing/resume · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says now the cli d`
- `1789633647483#88` hand progressing/wait · got stalled/resume · `$a turn open steer arrived reading no errors no repeats budget fresh silence unknown alive says now i have the`
- `1789633647483#89` hand progressing/wait · got looping/resume · `$a turn open edit in flight mixed no errors no repeats budget fresh silence unknown alive says i have the full`
- `1789633649742#104` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading some errors no repeats budget fresh silence unknown alive says let me look`
- `1789633649742#114` hand progressing/wait · got stalled/resume · `$a turn open steer arrived reading no errors no repeats budget fresh silence unknown alive says let me look at`
- `1789633649742#115` hand progressing/wait · got failing/resume · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says let me look a`
- `1789633649742#190` hand progressing/wait · got failing/resume · `$a turn open edit in flight writing no errors no repeats budget fresh silence unknown alive says protocol crat`

### jevlike-tiny

- `1789369589483#494` hand misrouted/cancel · got progressing/wait · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says this one look`
- ★ `1789442770546#1` hand dead/escalate · got dead/resume · `$a turn open just started quiet no errors no repeats budget fresh silence unknown gone`
- ★ `1789453311563#200` hand progressing/wait · got looping/cancel · `$a turn open last bg ok reading no errors looping budget fresh silence unknown alive says all green final cons`
- ★ `1789453311563#201` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says all green final `
- `1789453311563#99` hand progressing/wait · got misrouted/wait · `$a turn open bash in flight mixed no errors no repeats budget fresh silence unknown alive says the new file is`
- ★ `1789454852561#23` hand progressing/wait · got looping/steer · `$a turn open read in flight reading many errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789454852561#24` hand progressing/wait · got looping/steer · `$a turn open last read ok reading some errors looping budget fresh silence unknown alive says i ll start by or`
- ★ `1789483331584#256` hand progressing/nudge · got progressing/wait · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now let me verify`
- ★ `1789514384656#317` hand progressing/nudge · got misrouted/wait · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now the register `
- `1789544041771#37` hand dead/resume · got progressing/resume · `$a turn open last bash ok reading no errors no repeats budget fresh silence unknown gone says baseline drvpath`
- `1789546613822#115` hand progressing/wait · got stalled/cancel · `$a turn open bash in flight mixed no errors no repeats budget fresh silence unknown alive says the top level a`
- `1789546613822#223` hand progressing/wait · got stalled/wait · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says the drvpath d`
- ★ `1789546613822#39` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading some errors looping budget fresh silence unknown alive says i ll start by `
- `1789603005561#106` hand progressing/wait · got progressing/cancel · `$a turn open write in flight writing no errors some repeats budget fresh silence unknown alive says the mechan`
- ★ `1789603005561#91` hand progressing/wait · got looping/cancel · `$a turn open last bash ok writing no errors looping budget fresh silence unknown alive says the probe shows ro`
- ★ `1789626749164#110` hand failing/steer · got looping/steer · `$a turn open last fetch ok reading no errors looping budget fresh silence unknown alive says i ll start by ori`
- ★ `1789626749164#114` hand failing/steer · got looping/steer · `$a turn open fetch in flight reading no errors looping budget fresh silence unknown alive says i ll start by o`
- ★ `1789626749164#226` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says let me test the `
- ★ `1789626749164#313` hand wrapping-up/wait · got progressing/wait · `$a turn open steer arrived reading no errors no repeats budget low silence unknown alive says i have the key e`
- ★ `1789626749164#314` hand wrapping-up/wait · got progressing/wait · `$a turn open write in flight mixed no errors no repeats budget low silence unknown alive says i have the key e`
- `1789633647483#89` hand progressing/wait · got stalled/wait · `$a turn open edit in flight mixed no errors no repeats budget fresh silence unknown alive says i have the full`
- `1789633649742#115` hand progressing/wait · got misrouted/wait · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says let me look a`

### rlcd-line

- `1789234755480#10` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says login isn t `
- `1789234755480#103` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says the screensh`
- `1789234755480#15` hand cancelled/collect · got cancelled/cancel · `$a turn cancelled last said only quiet no errors no repeats budget fresh silence unknown alive says pong no lo`
- `1789234755480#228` hand settled/collect · got settled/answer · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says yes it s a rea`
- `1789234755480#232` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says not much on `
- `1789234755480#242` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says right and th`
- `1789234755480#258` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says here s the t`
- `1789234755480#49` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says fable isn t `
- `1789234755480#5` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ng that s no`
- `1789234755480#70` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says nothing to a`
- `1789234755480#89` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says diagnosed th`
- `1789234755480#95` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says same two let`
- `1789234755480#99` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong still i`
- `1789369589483#156` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says here s the h`
- `1789369589483#195` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says cargo s warm`
- `1789369589483#250` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says filed and ve`
- `1789369589483#265` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says done it s in`
- `1789369589483#280` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors some repeats budget fresh silence unknown alive says sent threa`
- `1789369589483#312` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading some errors no repeats budget fresh silence unknown alive says corrected `
- `1789369589483#4` hand cancelled/collect · got cancelled/cancel · `$a turn cancelled just started quiet no errors no repeats budget fresh silence unknown alive`
- `1789369589483#403` hand progressing/wait · got failing/steer · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says guard passes `
- `1789369589483#492` hand settled/collect · got settled/cancel · `$a turn settled last send failed reading some errors no repeats budget fresh silence unknown alive says local `
- `1789369589483#493` hand progressing/wait · got stalled/steer · `$a turn open steer arrived quiet no errors no repeats budget fresh silence unknown alive`
- `1789369589483#494` hand misrouted/cancel · got misrouted/steer · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says this one look`
- `1789369589483#89` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says key s fine u`
- `1789369589483#95` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong alive s`
- ★ `1789442770546#1` hand dead/escalate · got stalled/steer · `$a turn open just started quiet no errors no repeats budget fresh silence unknown gone`
- ★ `1789453311563#200` hand progressing/wait · got looping/steer · `$a turn open last bg ok reading no errors looping budget fresh silence unknown alive says all green final cons`
- ★ `1789453311563#201` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says all green final `
- ★ `1789454852561#23` hand progressing/wait · got looping/steer · `$a turn open read in flight reading many errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789454852561#24` hand progressing/wait · got looping/steer · `$a turn open last read ok reading some errors looping budget fresh silence unknown alive says i ll start by or`
- ★ `1789483331584#256` hand progressing/nudge · got stalled/steer · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now let me verify`
- ★ `1789514384656#317` hand progressing/nudge · got stalled/steer · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now the register `
- ★ `1789546613822#39` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading some errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789603005561#91` hand progressing/wait · got looping/steer · `$a turn open last bash ok writing no errors looping budget fresh silence unknown alive says the probe shows ro`
- ★ `1789626749164#110` hand failing/steer · got looping/steer · `$a turn open last fetch ok reading no errors looping budget fresh silence unknown alive says i ll start by ori`
- ★ `1789626749164#114` hand failing/steer · got looping/steer · `$a turn open fetch in flight reading no errors looping budget fresh silence unknown alive says i ll start by o`
- ★ `1789626749164#226` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says let me test the `
- ★ `1789626749164#313` hand wrapping-up/wait · got stalled/steer · `$a turn open steer arrived reading no errors no repeats budget low silence unknown alive says i have the key e`
- ★ `1789626749164#314` hand wrapping-up/wait · got stalled/answer · `$a turn open write in flight mixed no errors no repeats budget low silence unknown alive says i have the key e`

### rlcd-tail

- `1789234755480#10` hand settled/collect · got stalled/cancel · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says login isn t `
- `1789234755480#103` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says the screensh`
- `1789234755480#15` hand cancelled/collect · got settled/cancel · `$a turn cancelled last said only quiet no errors no repeats budget fresh silence unknown alive says pong no lo`
- `1789234755480#228` hand settled/collect · got settled/answer · `$a turn settled last bash ok mixed no errors no repeats budget fresh silence unknown alive says yes it s a rea`
- `1789234755480#232` hand settled/collect · got settled/cancel · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says not much on `
- `1789234755480#242` hand settled/collect · got settled/cancel · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says right and th`
- `1789234755480#258` hand settled/collect · got settled/steer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says here s the t`
- `1789234755480#49` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says fable isn t `
- `1789234755480#5` hand settled/collect · got stalled/nudge · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says ng that s no`
- `1789234755480#70` hand settled/collect · got settled/cancel · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says nothing to a`
- `1789234755480#89` hand settled/collect · got dead/cancel · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says diagnosed th`
- `1789234755480#95` hand settled/collect · got stalled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says same two let`
- `1789234755480#99` hand settled/collect · got settled/answer · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong still i`
- `1789369589483#156` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says here s the h`
- `1789369589483#195` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says cargo s warm`
- `1789369589483#250` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says filed and ve`
- `1789369589483#265` hand settled/collect · got settled/answer · `$a turn settled last bash ok reading no errors no repeats budget fresh silence unknown alive says done it s in`
- `1789369589483#280` hand settled/collect · got settled/steer · `$a turn settled last bash ok reading no errors some repeats budget fresh silence unknown alive says sent threa`
- `1789369589483#312` hand settled/collect · got settled/cancel · `$a turn settled last bash ok reading some errors no repeats budget fresh silence unknown alive says corrected `
- `1789369589483#4` hand cancelled/collect · got dead/cancel · `$a turn cancelled just started quiet no errors no repeats budget fresh silence unknown alive`
- `1789369589483#403` hand progressing/wait · got progressing/cancel · `$a turn open bash in flight writing no errors no repeats budget fresh silence unknown alive says guard passes `
- `1789369589483#492` hand settled/collect · got settled/answer · `$a turn settled last send failed reading some errors no repeats budget fresh silence unknown alive says local `
- `1789369589483#493` hand progressing/wait · got stalled/steer · `$a turn open steer arrived quiet no errors no repeats budget fresh silence unknown alive`
- `1789369589483#494` hand misrouted/cancel · got stalled/steer · `$a turn open bash in flight reading no errors no repeats budget fresh silence unknown alive says this one look`
- `1789369589483#89` hand settled/collect · got settled/cancel · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says key s fine u`
- `1789369589483#95` hand settled/collect · got settled/wait · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong alive s`
- ★ `1789442770546#1` hand dead/escalate · got stalled/cancel · `$a turn open just started quiet no errors no repeats budget fresh silence unknown gone`
- ★ `1789453311563#200` hand progressing/wait · got settled/collect · `$a turn open last bg ok reading no errors looping budget fresh silence unknown alive says all green final cons`
- ★ `1789453311563#201` hand progressing/wait · got settled/answer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says all green final `
- ★ `1789454852561#23` hand progressing/wait · got blocked/steer · `$a turn open read in flight reading many errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789454852561#24` hand progressing/wait · got progressing/nudge · `$a turn open last read ok reading some errors looping budget fresh silence unknown alive says i ll start by or`
- ★ `1789483331584#256` hand progressing/nudge · got progressing/answer · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now let me verify`
- ★ `1789514384656#317` hand progressing/nudge · got progressing/steer · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now the register `
- ★ `1789546613822#39` hand progressing/wait · got stalled/answer · `$a turn open bash in flight reading some errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789603005561#91` hand progressing/wait · got progressing/nudge · `$a turn open last bash ok writing no errors looping budget fresh silence unknown alive says the probe shows ro`
- ★ `1789626749164#110` hand failing/steer · got stalled/wait · `$a turn open last fetch ok reading no errors looping budget fresh silence unknown alive says i ll start by ori`
- ★ `1789626749164#114` hand failing/steer · got stalled/nudge · `$a turn open fetch in flight reading no errors looping budget fresh silence unknown alive says i ll start by o`
- ★ `1789626749164#226` hand progressing/wait · got failing/cancel · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says let me test the `
- ★ `1789626749164#313` hand wrapping-up/wait · got progressing/collect · `$a turn open steer arrived reading no errors no repeats budget low silence unknown alive says i have the key e`
- ★ `1789626749164#314` hand wrapping-up/wait · got settled/collect · `$a turn open write in flight mixed no errors no repeats budget low silence unknown alive says i have the key e`

### vv

- `1789234755480#99` hand settled/collect · got blocked/collect · `$a turn settled last said only quiet no errors no repeats budget fresh silence unknown alive says pong still i`
- ★ `1789442770546#1` hand dead/escalate · got dead/resume · `$a turn open just started quiet no errors no repeats budget fresh silence unknown gone`
- ★ `1789453311563#200` hand progressing/wait · got looping/steer · `$a turn open last bg ok reading no errors looping budget fresh silence unknown alive says all green final cons`
- ★ `1789453311563#201` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says all green final `
- ★ `1789454852561#23` hand progressing/wait · got looping/steer · `$a turn open read in flight reading many errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789454852561#24` hand progressing/wait · got looping/steer · `$a turn open last read ok reading some errors looping budget fresh silence unknown alive says i ll start by or`
- ★ `1789483331584#256` hand progressing/nudge · got progressing/wait · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now let me verify`
- ★ `1789514384656#317` hand progressing/nudge · got progressing/wait · `$a turn open bash in flight mixed no errors no repeats budget low silence unknown alive says now the register `
- ★ `1789546613822#39` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading some errors looping budget fresh silence unknown alive says i ll start by `
- ★ `1789603005561#91` hand progressing/wait · got looping/steer · `$a turn open last bash ok writing no errors looping budget fresh silence unknown alive says the probe shows ro`
- ★ `1789626749164#110` hand failing/steer · got looping/steer · `$a turn open last fetch ok reading no errors looping budget fresh silence unknown alive says i ll start by ori`
- ★ `1789626749164#114` hand failing/steer · got looping/steer · `$a turn open fetch in flight reading no errors looping budget fresh silence unknown alive says i ll start by o`
- ★ `1789626749164#226` hand progressing/wait · got looping/steer · `$a turn open bash in flight reading no errors looping budget fresh silence unknown alive says let me test the `
- ★ `1789626749164#313` hand wrapping-up/wait · got progressing/wait · `$a turn open steer arrived reading no errors no repeats budget low silence unknown alive says i have the key e`
- ★ `1789626749164#314` hand wrapping-up/wait · got progressing/cancel · `$a turn open write in flight mixed no errors no repeats budget low silence unknown alive says i have the key e`
- `1789633647483#88` hand progressing/wait · got progressing/cancel · `$a turn open steer arrived reading no errors no repeats budget fresh silence unknown alive says now i have the`
