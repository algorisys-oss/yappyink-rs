# gnome-magnifier: T037's probe

Does the GNOME Shell magnifier give yappyink live zoom without capture
(ADR-008)? A shell script, because the question is about the compositor, not
our code: it switches the Shell magnifier on through its settings while the
overlay runs, then off, and restores the user's values on every exit.

```sh
cargo build --release -p yappyink
experiments/gnome-magnifier/probe.sh            # 45 s at 2x
experiments/gnome-magnifier/probe.sh ./target/release/yappyink 30 3.0
```

## What to record, in E0NN

1. With zoom on, is the overlay magnified along with everything else?
2. Does the magnified view follow the pointer and keep updating (a video)?
3. Drawing while zoomed: does the stroke land under the pointer?
4. After zoom off: is the stroke still on the content it marked?
5. Are the settings restored (the script prints them)?
6. Any `[fault` lines in the overlay log?

Delete this directory when T037 closes, like the other experiments.
