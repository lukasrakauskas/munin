use rune::{AppSpec, Cx, El, TextEdit, col, row, text};
use rune_kit::{Kit, TextArea};

struct State {
    edit: TextEdit,
}

impl Default for State {
    fn default() -> Self {
        Self {
            edit: TextEdit::new(""),
        }
    }
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let k = Kit::neutral(cx.system_dark());
    row()
        .w_full()
        .h_full()
        .fill(k.c.page)
        .child(
            col()
                .key("sidebar")
                .w(220.0)
                .h_full()
                .padding(20.0)
                .fill(k.c.layer_01)
                .child(text("MUNIN").color(k.c.text_base))
                .child(text("Open a folder to begin").color(k.c.text_weak)),
        )
        .child(
            col()
                .key("writing-surface")
                .grow(1.0)
                .h_full()
                .padding(36.0)
                .child(
                    TextArea::new(&s.edit)
                        .width(None)
                        .rows(20)
                        .placeholder("Open a note to start writing")
                        .on_edit(|s: &mut State| &mut s.edit)
                        .build(&k),
                ),
        )
}

fn main() {
    rune::run(AppSpec::new("Munin", (1000.0, 700.0), State::default, view));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn empty_window_has_sidebar_and_writing_surface() {
        let mut rt = Runtime::new(
            State::default(),
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        assert!(rt.find("sidebar").is_some());
        assert!(rt.find("writing-surface").is_some());
    }
}
