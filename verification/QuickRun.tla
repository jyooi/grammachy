---- MODULE QuickRun ----
EXTENDS Naturals

CONSTANTS MaxSummons, MaxClears

VARIABLES
  opened, phase, gen, summons, clears,
  runCaptured, replacePending, applied, capturedText,
  clipboardBorrowed, borrowed, pendingPrimaryPaste, sourceWindow,
  keptText, keptWindow,
  proc, settle, restore, rc, pasteAfter,
  kind, clipboard, primary, sourceOpen, focus,
  took, releases, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead

overlay == <<opened, phase, gen, runCaptured, replacePending, applied, capturedText,
             clipboardBorrowed, borrowed, pendingPrimaryPaste, sourceWindow,
             keptText, keptWindow, pasteAfter>>

processes == <<proc, settle, restore, rc>>

desktop == <<kind, clipboard, primary, sourceOpen, focus>>

counters == <<summons, clears>>

ghosts == <<took, releases, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

vars == <<overlay, processes, desktop, counters, ghosts>>

Slots == {"probe", "primary", "borrow", "keys", "fallback", "check"}

Windows == {"source", "other", "desktop"}

Texts == {"mine", "sel", "fix", ""}

Phases == {"capturing", "checking", "result", "error", "empty", "notice"}

Idle == [st |-> "idle", sg |-> 0, again |-> FALSE]

LaunchRec(r, g) ==
  IF r.st = "idle" THEN [st |-> "launched", sg |-> g, again |-> FALSE] ELSE [r EXCEPT !.again = TRUE]

KillRec(r) == [r EXCEPT !.again = FALSE]

FinishRec(r) == IF r.again THEN [st |-> "launched", sg |-> gen, again |-> FALSE] ELSE Idle

Live(s) == proc[s].sg = gen

RcIdle == [step |-> "idle", gen |-> 0, window |-> "none", took |-> "none", closed |-> FALSE]

SettleOff == [on |-> FALSE, gen |-> 0]

RestoreIdle == [st |-> "idle", again |-> FALSE, text |-> ""]

RestoreLaunch(text) ==
  IF restore.st = "idle"
    THEN [st |-> "launched", again |-> FALSE, text |-> text]
    ELSE [restore EXCEPT !.again = TRUE, !.text = text]

Visible == opened /\ phase # "capturing"

Target == IF Visible THEN "popup" ELSE focus

IsSelection(text) == text # ""

CopyLands == IF Target = "source" /\ sourceOpen /\ kind = "field" THEN "sel" ELSE clipboard

UserMayAct == proc["keys"].st = "idle" /\ rc.step # "paste"

Quiet ==
  /\ \A s \in Slots : proc[s].st = "idle"
  /\ restore.st = "idle"
  /\ ~settle.on
  /\ rc.step = "idle"

Release(pending) ==
  IF runCaptured /\ ~pending
    THEN /\ runCaptured' = FALSE
         /\ primary' = ""
         /\ releases' = releases + 1
    ELSE UNCHANGED <<runCaptured, primary, releases>>

Consume ==
  IF runCaptured /\ capturedText # ""
    THEN /\ keptText' = capturedText
         /\ keptWindow' = sourceWindow
    ELSE UNCHANGED <<keptText, keptWindow>>

ShowNothingNew(p) ==
  /\ phase' = "empty"
  /\ capturedText' = ""
  /\ proc' = p
  /\ UNCHANGED <<runCaptured, took, keptText, keptWindow, applied>>

Captured(text, from, p) ==
  IF keptText # "" /\ text = keptText /\ sourceWindow = keptWindow
    THEN ShowNothingNew(p)
    ELSE /\ phase' = "checking"
         /\ capturedText' = text
         /\ runCaptured' = TRUE
         /\ took' = from
         /\ keptText' = text
         /\ keptWindow' = sourceWindow
         /\ applied' = FALSE
         /\ proc' = [p EXCEPT !["check"] = LaunchRec(@, gen)]

Init ==
  /\ opened = FALSE
  /\ phase = "capturing"
  /\ gen = 0
  /\ summons = 0
  /\ clears = 0
  /\ runCaptured = FALSE
  /\ replacePending = FALSE
  /\ applied = FALSE
  /\ capturedText = ""
  /\ clipboardBorrowed = FALSE
  /\ borrowed = ""
  /\ pendingPrimaryPaste = FALSE
  /\ sourceWindow = "none"
  /\ keptText = ""
  /\ keptWindow = "none"
  /\ proc = [s \in Slots |-> Idle]
  /\ settle = SettleOff
  /\ restore = RestoreIdle
  /\ rc = RcIdle
  /\ pasteAfter = FALSE
  /\ kind \in {"terminal", "field", "bare"}
  /\ clipboard = "mine"
  /\ primary = IF kind = "terminal" THEN "sel" ELSE ""
  /\ sourceOpen = TRUE
  /\ focus = "source"
  /\ took = "none"
  /\ releases = 0
  /\ strayed = FALSE
  /\ highlightLost = FALSE
  /\ highlightLostEarly = FALSE
  /\ pendingGen = 0
  /\ staleRead = FALSE

YouSummon ==
  /\ UserMayAct
  /\ summons < MaxSummons
  /\ summons' = summons + 1
  /\ opened' = TRUE
  /\ gen' = gen + 1
  /\ proc' = [s \in Slots |-> IF s = "probe" THEN LaunchRec(KillRec(proc[s]), gen + 1) ELSE KillRec(proc[s])]
  /\ settle' = SettleOff
  /\ rc' = IF rc.step \in {"timer", "focus", "verify"} THEN RcIdle ELSE rc
  /\ pasteAfter' = FALSE
  /\ replacePending' = FALSE
  /\ runCaptured' = FALSE
  /\ applied' = FALSE
  /\ sourceWindow' = "none"
  /\ restore' = IF clipboardBorrowed /\ restore.st = "idle" THEN RestoreLaunch(borrowed) ELSE restore
  /\ clipboardBorrowed' = FALSE
  /\ borrowed' = ""
  /\ phase' = "capturing"
  /\ capturedText' = ""
  /\ took' = "none"
  /\ releases' = 0
  /\ UNCHANGED <<clears, pendingPrimaryPaste, keptText, keptWindow,
                 desktop, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

ProbeAnswers ==
  /\ proc["probe"].st = "launched"
  /\ LET p1 == [proc EXCEPT !["probe"] = FinishRec(@)]
     IN IF ~Live("probe")
          THEN /\ proc' = p1
               /\ UNCHANGED <<sourceWindow, pendingPrimaryPaste, pendingGen>>
          ELSE /\ sourceWindow' = IF focus = "desktop" THEN "none" ELSE focus
               /\ IF restore.st # "idle"
                    THEN /\ pendingPrimaryPaste' = TRUE
                         /\ pendingGen' = gen
                         /\ proc' = p1
                    ELSE /\ pendingPrimaryPaste' = FALSE
                         /\ proc' = [p1 EXCEPT !["primary"] = LaunchRec(@, gen)]
                         /\ UNCHANGED pendingGen
  /\ UNCHANGED <<opened, phase, gen, runCaptured, replacePending, applied, capturedText,
                 clipboardBorrowed, borrowed, keptText, keptWindow, pasteAfter,
                 settle, restore, rc, desktop, counters,
                 took, releases, strayed, highlightLost, highlightLostEarly, staleRead>>

PrimaryAnswers ==
  /\ proc["primary"].st = "launched"
  /\ LET p1 == [proc EXCEPT !["primary"] = FinishRec(@)]
     IN IF Live("primary") /\ IsSelection(primary)
          THEN Captured(primary, "primary", p1)
          ELSE /\ proc' = IF Live("primary") THEN [p1 EXCEPT !["borrow"] = LaunchRec(@, gen)] ELSE p1
               /\ UNCHANGED <<phase, capturedText, runCaptured, took, keptText, keptWindow, applied>>
  /\ UNCHANGED <<opened, gen, replacePending, clipboardBorrowed, borrowed, pendingPrimaryPaste,
                 sourceWindow, pasteAfter, settle, restore, rc, desktop, counters,
                 releases, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

BorrowAnswers ==
  /\ proc["borrow"].st = "launched"
  /\ LET p1 == [proc EXCEPT !["borrow"] = FinishRec(@)]
     IN IF Live("borrow")
          THEN /\ borrowed' = clipboard
               /\ clipboardBorrowed' = TRUE
               /\ proc' = [p1 EXCEPT !["keys"] = LaunchRec(@, gen)]
          ELSE /\ proc' = p1
               /\ UNCHANGED <<borrowed, clipboardBorrowed>>
  /\ UNCHANGED <<opened, phase, gen, runCaptured, replacePending, applied, capturedText,
                 pendingPrimaryPaste, sourceWindow, keptText, keptWindow, pasteAfter,
                 settle, restore, rc, desktop, counters, ghosts>>

KeysSent ==
  /\ proc["keys"].st = "launched"
  /\ clipboard' = CopyLands
  /\ settle' = IF Live("keys") THEN [on |-> TRUE, gen |-> gen] ELSE settle
  /\ proc' = [proc EXCEPT !["keys"] = FinishRec(@)]
  /\ UNCHANGED <<overlay, restore, rc, kind, primary, sourceOpen, focus, counters, ghosts>>

SettleEnds ==
  /\ settle.on
  /\ settle' = SettleOff
  /\ proc' = IF settle.gen = gen THEN [proc EXCEPT !["fallback"] = LaunchRec(@, gen)] ELSE proc
  /\ UNCHANGED <<overlay, restore, rc, desktop, counters, ghosts>>

FallbackAnswers ==
  /\ proc["fallback"].st = "launched"
  /\ LET p1 == [proc EXCEPT !["fallback"] = FinishRec(@)]
     IN IF Live("fallback")
          THEN /\ clipboardBorrowed' = FALSE
               /\ restore' = IF clipboardBorrowed THEN RestoreLaunch(borrowed) ELSE restore
               /\ IF borrowed = clipboard \/ ~IsSelection(clipboard)
                    THEN ShowNothingNew(p1)
                    ELSE Captured(clipboard, "copy", p1)
          ELSE /\ proc' = p1
               /\ UNCHANGED <<phase, capturedText, runCaptured, took, keptText, keptWindow, applied,
                              clipboardBorrowed, restore>>
  /\ UNCHANGED <<opened, gen, replacePending, borrowed, pendingPrimaryPaste, sourceWindow,
                 pasteAfter, settle, rc, desktop, counters,
                 releases, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

RestoreWrites ==
  /\ restore.st = "launched"
  /\ clipboard' = restore.text
  /\ restore' = IF restore.again THEN [restore EXCEPT !.again = FALSE] ELSE RestoreIdle
  /\ IF pendingPrimaryPaste
       THEN /\ pendingPrimaryPaste' = FALSE
            /\ proc' = [proc EXCEPT !["primary"] = LaunchRec(@, gen)]
            /\ staleRead' = (staleRead \/ pendingGen # gen)
            /\ pendingGen' = 0
       ELSE UNCHANGED <<pendingPrimaryPaste, proc, staleRead, pendingGen>>
  /\ UNCHANGED <<opened, phase, gen, runCaptured, replacePending, applied, capturedText,
                 clipboardBorrowed, borrowed, sourceWindow, keptText, keptWindow, pasteAfter,
                 settle, rc, kind, primary, sourceOpen, focus, counters,
                 took, releases, strayed, highlightLost, highlightLostEarly>>

CheckAnswers ==
  /\ proc["check"].st = "launched"
  /\ \E answer \in {"result", "error"} :
       phase' = IF Live("check") THEN answer ELSE phase
  /\ proc' = [proc EXCEPT !["check"] = FinishRec(@)]
  /\ UNCHANGED <<opened, gen, runCaptured, replacePending, applied, capturedText,
                 clipboardBorrowed, borrowed, pendingPrimaryPaste, sourceWindow,
                 keptText, keptWindow, pasteAfter, settle, restore, rc,
                 desktop, counters, ghosts>>

YouClose ==
  /\ UserMayAct
  /\ Visible
  /\ opened' = FALSE
  /\ Consume
  /\ Release(replacePending)
  /\ rc' = IF rc.step = "copy" /\ pasteAfter THEN [rc EXCEPT !.closed = TRUE] ELSE rc
  /\ UNCHANGED <<phase, gen, replacePending, applied, capturedText,
                 clipboardBorrowed, borrowed, pendingPrimaryPaste, sourceWindow, pasteAfter,
                 proc, settle, restore, kind, clipboard, sourceOpen, focus, counters,
                 took, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

YouClear ==
  /\ UserMayAct
  /\ Visible
  /\ phase \in {"checking", "error", "notice", "result"}
  /\ clears < MaxClears
  /\ clears' = clears + 1
  /\ gen' = gen + 1
  /\ proc' = [proc EXCEPT !["check"] = KillRec(@)]
  /\ applied' = FALSE
  /\ restore' = IF clipboardBorrowed /\ restore.st = "idle" THEN RestoreLaunch(borrowed) ELSE restore
  /\ clipboardBorrowed' = FALSE
  /\ borrowed' = ""
  /\ Release(replacePending)
  /\ phase' = "empty"
  /\ capturedText' = ""
  /\ UNCHANGED <<opened, replacePending, pendingPrimaryPaste, sourceWindow,
                 keptText, keptWindow, pasteAfter, settle, rc,
                 kind, clipboard, sourceOpen, focus, summons,
                 took, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

YouCheckLastAgain ==
  /\ UserMayAct
  /\ Visible
  /\ phase = "empty"
  /\ keptText # ""
  /\ capturedText' = keptText
  /\ phase' = "checking"
  /\ applied' = FALSE
  /\ proc' = [proc EXCEPT !["check"] = LaunchRec(@, gen)]
  /\ UNCHANGED <<opened, gen, runCaptured, replacePending,
                 clipboardBorrowed, borrowed, pendingPrimaryPaste, sourceWindow,
                 keptText, keptWindow, pasteAfter, settle, restore, rc,
                 desktop, counters, ghosts>>

YouApply ==
  /\ UserMayAct
  /\ Visible
  /\ phase = "result"
  /\ ~applied
  /\ rc.step = "idle"
  /\ \E replace \in {FALSE, runCaptured} : pasteAfter' = replace
  /\ rc' = [rc EXCEPT !.step = "copy", !.gen = gen, !.closed = FALSE]
  /\ applied' = TRUE
  /\ UNCHANGED <<opened, phase, gen, runCaptured, replacePending, capturedText,
                 clipboardBorrowed, borrowed, pendingPrimaryPaste, sourceWindow,
                 keptText, keptWindow, proc, settle, restore, desktop, counters, ghosts>>

CopyEnds ==
  /\ rc.step = "copy"
  /\ clipboard' = "fix"
  /\ pasteAfter' = FALSE
  /\ IF pasteAfter /\ rc.gen = gen
       THEN /\ replacePending' = TRUE
            /\ opened' = FALSE
            /\ Consume
            /\ Release(TRUE)
            /\ rc' = [rc EXCEPT !.step = "timer"]
       ELSE /\ rc' = RcIdle
            /\ UNCHANGED <<replacePending, opened, keptText, keptWindow, runCaptured, primary, releases>>
  /\ UNCHANGED <<phase, gen, applied, capturedText, clipboardBorrowed, borrowed,
                 pendingPrimaryPaste, sourceWindow, proc, settle, restore,
                 kind, sourceOpen, focus, counters,
                 took, strayed, highlightLost, highlightLostEarly, pendingGen, staleRead>>

PasteWaitEnds ==
  /\ rc.step = "timer"
  /\ rc' = CASE rc.gen # gen -> RcIdle
             [] sourceWindow = "none" -> [rc EXCEPT !.step = "paste", !.window = "none", !.took = took]
             [] OTHER -> [rc EXCEPT !.step = "focus"]
  /\ UNCHANGED <<overlay, proc, settle, restore, desktop, counters, ghosts>>

FocusDispatched ==
  /\ rc.step = "focus"
  /\ focus' = IF sourceWindow = "other" \/ (sourceWindow = "source" /\ sourceOpen)
                THEN sourceWindow
                ELSE focus
  /\ rc' = IF rc.gen = gen THEN [rc EXCEPT !.step = "verify"] ELSE RcIdle
  /\ UNCHANGED <<overlay, proc, settle, restore, kind, clipboard, primary, sourceOpen,
                 counters, ghosts>>

FocusVerified ==
  /\ rc.step = "verify"
  /\ IF rc.gen = gen /\ focus = sourceWindow
       THEN /\ rc' = [rc EXCEPT !.step = "paste", !.window = sourceWindow, !.took = took]
            /\ UNCHANGED <<replacePending, opened, phase>>
       ELSE IF rc.gen = gen
         THEN /\ rc' = RcIdle
              /\ replacePending' = FALSE
              /\ opened' = TRUE
              /\ phase' = "notice"
         ELSE /\ rc' = RcIdle
              /\ UNCHANGED <<replacePending, opened, phase>>
  /\ UNCHANGED <<gen, runCaptured, applied, capturedText, clipboardBorrowed, borrowed,
                 pendingPrimaryPaste, sourceWindow, keptText, keptWindow, pasteAfter,
                 proc, settle, restore, desktop, counters, ghosts>>

PasteTyped ==
  /\ rc.step = "paste"
  /\ strayed' = (strayed \/ (rc.window # "none" /\ Target \notin {rc.window, "popup"}))
  /\ highlightLostEarly' = (highlightLostEarly \/ (rc.took = "primary" /\ releases > 0))
  /\ highlightLost' = (highlightLost \/ (rc.took = "primary" /\ releases > 0 /\ ~rc.closed))
  /\ replacePending' = FALSE
  /\ IF rc.gen = gen
       THEN Release(FALSE)
       ELSE UNCHANGED <<runCaptured, primary, releases>>
  /\ rc' = RcIdle
  /\ UNCHANGED <<opened, phase, gen, applied, capturedText, clipboardBorrowed, borrowed,
                 pendingPrimaryPaste, sourceWindow, keptText, keptWindow, pasteAfter,
                 proc, settle, restore, kind, clipboard, sourceOpen, focus, counters,
                 took, pendingGen, staleRead>>

FocusMoves ==
  /\ UserMayAct
  /\ ~Visible
  /\ \E w \in Windows \ {focus} :
       /\ w # "source" \/ sourceOpen
       /\ focus' = w
  /\ UNCHANGED <<overlay, processes, kind, clipboard, primary, sourceOpen, counters, ghosts>>

SourceCloses ==
  /\ UserMayAct
  /\ sourceOpen
  /\ sourceOpen' = FALSE
  /\ primary' = ""
  /\ \E w \in {"other", "desktop"} : focus' = IF focus = "source" THEN w ELSE focus
  /\ UNCHANGED <<overlay, processes, kind, clipboard, counters, ghosts>>

Finished ==
  /\ Quiet
  /\ UNCHANGED vars

Next ==
  \/ YouSummon
  \/ ProbeAnswers
  \/ PrimaryAnswers
  \/ BorrowAnswers
  \/ KeysSent
  \/ SettleEnds
  \/ FallbackAnswers
  \/ RestoreWrites
  \/ CheckAnswers
  \/ YouClose
  \/ YouClear
  \/ YouCheckLastAgain
  \/ YouApply
  \/ CopyEnds
  \/ PasteWaitEnds
  \/ FocusDispatched
  \/ FocusVerified
  \/ PasteTyped
  \/ FocusMoves
  \/ SourceCloses
  \/ Finished

Spec == Init /\ [][Next]_vars

TypeOK ==
  /\ phase \in Phases
  /\ clipboard \in Texts
  /\ borrowed \in Texts
  /\ primary \in {"sel", ""}
  /\ focus \in Windows
  /\ sourceWindow \in {"none", "source", "other"}
  /\ releases \in 0..2
  /\ \A s \in Slots : proc[s].st \in {"idle", "launched"}

CardShows == Quiet => ~(opened /\ phase = "capturing")

PasteNeverStrays == ~strayed

ClipboardComesBack == Quiet => clipboard \in {"mine", "fix"}

ClipboardComesBackWithoutStaleRead == ~staleRead => ClipboardComesBack

ReleaseOnce == releases <= 1 /\ (releases = 1 => took # "none")

ReleaseOnceWithoutStaleRead == ~staleRead => ReleaseOnce

HighlightOutlivesReplace == ~highlightLostEarly

HighlightOutlivesReplaceWithoutEarlyClose == ~highlightLost
====
