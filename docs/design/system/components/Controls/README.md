Every control is a real button, link, input, label or form; the browser is a display engine and the form is the whole of the protocol it holds.

- A verb is one form with one `.verb` button whose text is the verb's word in `id`, 36px tall. The consumer provides the verbs the person's roles carry and the ceiling grants; every other verb is rendered disabled with a note saying why, never hidden.
- A turn is a `textarea` in `mono` with a `.verb.primary` button reading "Place the turn". When more than one enabled person holds `turn`, the label carries the shared-conversation note.
- `.primary` is the one action on a pane, bordered in `accent` on `accent-soft`. There is at most one per pane.
- Inputs and selects are 32px on `inset` with a `line` border; the focus ring is `focus`, 2px, offset 2px.
- A row's several actions are forms side by side in its cell (`td > form`), a secret is a password input drawn as the text input is, a turn's `textarea` fills its pane's width, and a button in a pane grows from its control height to hold words that wrap (a long decision option, a config's file name); all are rules of `bundle.css`'s served-surfaces section.
- A field whose mistake is caught now carries `.when.now`; one caught only at load carries `.when.load`. The consumer derives this from who holds custody of the field; it is never authored by hand.
