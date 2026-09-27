// The Lighthouse Keeper's Ledger — the Ink original being ported.
VAR oil = 1
VAR knows_name = false
VAR lamp_lit = false

-> arrival

=== arrival ===
The boat leaves you on the rock at dusk. The keeper's door is unlocked.
* [Go inside] -> lamp_room

=== lamp_room ===
{lamp_room == 1: The lamp room smells of cold brass.|The lamp room again. The dark is closer.}
{oil > 0: A can of oil sits by the lens.}
+ [Read the ledger] -> ledger
+ {ledger > 0} [Search the keeper's desk] -> desk
+ [Look out from the gallery] -> gallery
* {not stores} [Go down to the stores] -> stores
+ {knows_name} [Say the keeper's name aloud] -> name_spoken
+ [Wait for dark] -> dusk

=== ledger ===
{ledger:
- 1: The last entry is three weeks old. "Oil low. Ship due."
- 2: You read it again. The handwriting shakes toward the end.
- else: The words have stopped changing.
}
-> lamp_room

=== desk ===
Under the blotter: a letter signed "Tomas Vell".
~ knows_name = true
-> lamp_room

=== gallery ===
{gallery > 2: A light on the water. A ship, coming in blind.|Only fog.}
-> lamp_room

=== stores ===
You find two more cans of oil.
~ oil = oil + 2
-> lamp_room

=== name_spoken ===
"Tomas." The lens turns a quarter, by itself.
-> lamp_room

=== dusk ===
{oil >= 3 && knows_name: -> ending_lit | -> ending_dark}

=== ending_lit ===
The lamp catches. Out on the water the ship turns.
-> END

=== ending_dark ===
The wick sputters. Somewhere below, timber meets rock.
-> END
