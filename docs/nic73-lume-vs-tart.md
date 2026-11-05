# NIC-73 macOS VM choice

NIC-73 needs one disposable macOS VM on Studio that can become an independent Mother. The VM manager only creates and drives the guest. It does not supply MCT identity or authority.

| Check | Lume | Tart |
| --- | --- | --- |
| License | [MIT](https://github.com/trycua/cua/blob/main/LICENSE.md). | [FSL-1.1-ALv2](https://github.com/openai/tart/blob/main/LICENSE), with internal use allowed and Apache 2.0 applying to a release after two years. |
| Copies and registry | `lume clone` copies the disk and NVRAM together. `lume pull` and `lume push` use OCI-compatible registries. The unattended Tahoe preset can create a fresh SSH-ready guest from an Apple restore image. | Local and remote `clone`, `pull`, and `push` use OCI-compatible registries. Tart also supports immutable stacked base disks on macOS 27 or newer. Cirrus Labs publishes prepared macOS images. |
| Agent driving | The CLI can inspect, start, stop, copy files, and run SSH commands. Lume also offers localhost HTTP and MCP control, but NIC-73 does not need or enable either service. | The CLI is mature for CI. Prepared guests can use `tart exec` through the Tart Guest Agent without guest networking. Tart also documents Packer and CI integrations. |
| Disposable-VM fit | A fresh unattended guest is small enough for one-node enrollment and easy to drive without adding a service. | Stronger existing CI image ecosystem and guest-agent execution, but those features are not required to enroll one Mother. |

Apple's [macOS Sequoia license](https://www.apple.com/legal/sla/docs/macOSSequoia.pdf), section 2B(iii), permits up to two additional macOS copies or instances in virtual operating-system environments on each Apple-branded computer already running macOS for software development, development testing, macOS Server, or personal non-commercial use. NIC-73 enrolls one VM and leaves the second slot unused. This is an operational reading, not legal advice.

## Decision

Enroll one **Lume** VM. Lume has the simpler license for repository-owned automation, a Homebrew package, unattended macOS setup, local CLI and SSH control, and OCI image support. Tart remains a good CI-oriented alternative, but its guest agent, published Xcode images, and stacked registry disks do not improve this enrollment slice enough to offset the more restrictive current license.

The VM starts from a fresh guest or an identity-free base image. Cloning a VM after Mother enrollment would also clone its private root and endpoint key, so it is forbidden. The enrolled guest creates its own private Mother root and endpoint key. Its NAT address is used only for Studio-to-guest administration. Enrollment is proved separately by an Iroh endpoint ticket, receiver-issued peer bindings and proofs, Child admission, and a caller-scoped directory-read Toy grant.

Sources:

- [Lume quickstart](https://cua.ai/docs/lume/quickstart)
- [Lume VM management](https://cua.ai/docs/lume/guides/manage-vms)
- [Lume CLI reference](https://cua.ai/docs/lume/reference/cli)
- [Tart quick start](https://tart.run/quick-start/)
- [Tart Guest Agent](https://tart.run/blog/2025/06/01/tart-guest-agent/)
