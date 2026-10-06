Currently works only with Pulse Audio rerouting to capture system audio.
So it only works on Linux and pactl.
The project uses then cpal for the audio playback.

> [!Note]
> It is very likely, that this wont work correctly on your machine, even though you are using pulse audio. That is most likely because of my audio sink module that needs adjustments to be more universal. Currently working on a fix. Starting the application and manually rerouting the audio signals could make it work.

Another addition, to let it really feel apart from scope-tui, is an FIR-filter with custom input for the length $n$ and parameters ($\alpha$ and $\beta$).
This can then also be shown as an transition function of the system on a separate popup or something to tweak the parameters and then see the spectrum of the filter also in a separate popup ...
This filter is of course toggleable and applied on the audio feedback.
The work in progress for this is currently in the test_fkts.rs file.
