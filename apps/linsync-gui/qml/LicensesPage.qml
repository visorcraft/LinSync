// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.Page {
    id: page
    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeBgLift: Qt.darker(Kirigami.Theme.alternateBackgroundColor, 1.06)
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor
    readonly property color themeHighlightedText: Kirigami.Theme.highlightedTextColor

    background: Rectangle { color: page.themeBg }

    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)
    readonly property color separatorStrong: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.35)

    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    palette.window:          page.themeBg
    palette.windowText:      page.themeText
    palette.base:            page.themeBg
    palette.alternateBase:   page.themeBgAlt
    palette.text:            page.themeText
    palette.button:          page.themeBgAlt
    palette.buttonText:      page.themeText
    palette.brightText:      page.themeHighlightedText
    palette.highlight:       page.themeHighlight
    palette.highlightedText: page.themeHighlightedText
    palette.mid:             page.separator
    palette.midlight:        page.themeBgAlt
    palette.light:           page.themeBgLift
    palette.dark:            page.themeBg
    palette.placeholderText: Qt.rgba(page.themeText.r, page.themeText.g, page.themeText.b, 0.55)

    GplLicenseText { id: gplLicense }

    property int activeDocument: 0
    property string filterText: ""
    property bool wrapText: false

    // Reset state to defaults when navigated away from
    // Component.onCompleted leaves defaults.

    readonly property string thirdPartyText:
        "<!-- SPDX-FileCopyrightText: 2026 VisorCraft LLC -->\n" +
        "<!-- SPDX-License-Identifier: GPL-3.0-only -->\n" +
        "# Third-Party Licenses\n" +
        "\n" +
        "This document lists the third-party Rust crates included in this build\n" +
        "of LinSync, grouped by license type.  The full license text is reproduced\n" +
        "under \"License Texts\".  All crates are fetched from crates.io and we\n" +
        "acknowledge their authors and copyright holders individually.\n" +
        "\n" +
        "If you have questions about license compliance, please contact\n" +
        "licensing@visorcraft.com -- regenerate this list with `just credits`\n" +
        "after any dependency change.\n" +
        "\n" +
        // BEGIN GENERATED CREDITS COUNTS (do not edit inside; run `just credits-update`)
        " - MIT License                  (115 crates)\n" +
        " - Apache License 2.0           (7 crates)\n" +
        " - BSD 3-Clause                 (2 crates)\n" +
        " - BSD 2-Clause                 (1 crate)\n" +
        " - Zlib                         (1 crate)\n" +
        // END GENERATED CREDITS COUNTS
        " - Unicode-3.0                   (applies to unicode-ident)\n" +
        " - Unlicense / CC0-1.0 / MIT-0   (public-domain equivalents)\n" +
        "\n" +
        "## Cargo Dependencies\n" +
        "\n" +
        "| Package              | Version  | License expression |\n" +
        "| -------------------- | -------- | ------------------ |\n" +
        // BEGIN GENERATED CREDITS TABLE (do not edit inside; run `just credits-update`)
        "| adler2               | 2.0.1    | 0BSD OR MIT OR Apache-2.0 |\n" +
        "| aho-corasick         | 1.1.4    | Unlicense OR MIT |\n" +
        "| anyhow               | 1.0.102  | MIT OR Apache-2.0 |\n" +
        "| arrayref             | 0.3.9    | BSD-2-Clause |\n" +
        "| arrayvec             | 0.7.6    | MIT OR Apache-2.0 |\n" +
        "| autocfg              | 1.5.1    | Apache-2.0 OR MIT |\n" +
        "| bincode              | 1.3.3    | MIT |\n" +
        "| bit_field            | 0.10.3   | Apache-2.0/MIT |\n" +
        "| bitflags             | 2.11.1   | MIT OR Apache-2.0 |\n" +
        "| blake3               | 1.8.5    | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |\n" +
        "| block-buffer         | 0.10.4   | MIT OR Apache-2.0 |\n" +
        "| bytemuck             | 1.25.0   | Zlib OR Apache-2.0 OR MIT |\n" +
        "| byteorder-lite       | 0.1.0    | Unlicense OR MIT |\n" +
        "| cc                   | 1.2.62   | MIT OR Apache-2.0 |\n" +
        "| cfg-if               | 1.0.4    | MIT OR Apache-2.0 |\n" +
        "| clang-format         | 0.3.0    | MIT OR Apache-2.0 |\n" +
        "| codespan-reporting   | 0.11.1   | Apache-2.0 |\n" +
        "| codespan-reporting   | 0.13.1   | Apache-2.0 |\n" +
        "| color_quant          | 1.1.0    | MIT |\n" +
        "| constant_time_eq     | 0.4.2    | CC0-1.0 OR MIT-0 OR Apache-2.0 |\n" +
        "| convert_case         | 0.6.0    | MIT |\n" +
        "| cpufeatures          | 0.2.17   | MIT OR Apache-2.0 |\n" +
        "| cpufeatures          | 0.3.0    | MIT OR Apache-2.0 |\n" +
        "| crc32fast            | 1.5.0    | MIT OR Apache-2.0 |\n" +
        "| crypto-common        | 0.1.7    | MIT OR Apache-2.0 |\n" +
        "| cxx                  | 1.0.194  | MIT OR Apache-2.0 |\n" +
        "| cxxbridge-flags      | 1.0.194  | MIT OR Apache-2.0 |\n" +
        "| cxxbridge-macro      | 1.0.194  | MIT OR Apache-2.0 |\n" +
        "| cxx-gen              | 0.7.194  | MIT OR Apache-2.0 |\n" +
        "| cxx-qt               | 0.8.1    | MIT OR Apache-2.0 |\n" +
        "| cxx-qt-build         | 0.8.1    | MIT OR Apache-2.0 |\n" +
        "| cxx-qt-gen           | 0.8.1    | MIT OR Apache-2.0 |\n" +
        "| cxx-qt-lib           | 0.8.1    | MIT OR Apache-2.0 |\n" +
        "| cxx-qt-macro         | 0.8.1    | MIT OR Apache-2.0 |\n" +
        "| digest               | 0.10.7   | MIT OR Apache-2.0 |\n" +
        "| enumflags2           | 0.7.12   | MIT OR Apache-2.0 |\n" +
        "| enumflags2_derive    | 0.7.12   | MIT OR Apache-2.0 |\n" +
        "| equivalent           | 1.0.2    | Apache-2.0 OR MIT |\n" +
        "| exr                  | 1.74.0   | BSD-3-Clause |\n" +
        "| fax                  | 0.2.7    | MIT |\n" +
        "| fdeflate             | 0.3.7    | MIT OR Apache-2.0 |\n" +
        "| find-msvc-tools      | 0.1.9    | MIT OR Apache-2.0 |\n" +
        "| flate2               | 1.1.9    | MIT OR Apache-2.0 |\n" +
        "| fnv                  | 1.0.7    | Apache-2.0 / MIT |\n" +
        "| foldhash             | 0.2.0    | Zlib |\n" +
        "| generic-array        | 0.14.7   | MIT |\n" +
        "| gif                  | 0.14.2   | MIT OR Apache-2.0 |\n" +
        "| half                 | 2.7.1    | MIT OR Apache-2.0 |\n" +
        "| hashbrown            | 0.17.1   | MIT OR Apache-2.0 |\n" +
        "| image                | 0.25.10  | MIT OR Apache-2.0 |\n" +
        "| image-webp           | 0.2.4    | MIT OR Apache-2.0 |\n" +
        "| indexmap             | 2.14.0   | Apache-2.0 OR MIT |\n" +
        "| indoc                | 2.0.7    | MIT OR Apache-2.0 |\n" +
        "| itoa                 | 1.0.18   | MIT OR Apache-2.0 |\n" +
        "| jobserver            | 0.1.34   | MIT OR Apache-2.0 |\n" +
        "| lab                  | 0.11.0   | MIT |\n" +
        "| landlock             | 0.4.5    | MIT OR Apache-2.0 |\n" +
        "| lazy_static          | 1.5.0    | MIT OR Apache-2.0 |\n" +
        "| lebe                 | 0.5.3    | BSD-3-Clause |\n" +
        "| libc                 | 0.2.186  | MIT OR Apache-2.0 |\n" +
        "| link-cplusplus       | 1.0.12   | MIT OR Apache-2.0 |\n" +
        "| log                  | 0.4.29   | MIT OR Apache-2.0 |\n" +
        "| memchr               | 2.8.0    | Unlicense OR MIT |\n" +
        "| miniz_oxide          | 0.8.9    | MIT OR Zlib OR Apache-2.0 |\n" +
        "| moxcms               | 0.8.1    | BSD-3-Clause OR Apache-2.0 |\n" +
        "| nu-ansi-term         | 0.50.3   | MIT |\n" +
        "| num-traits           | 0.2.19   | MIT OR Apache-2.0 |\n" +
        "| once_cell            | 1.21.4   | MIT OR Apache-2.0 |\n" +
        "| onig                 | 6.5.3    | MIT |\n" +
        "| onig_sys             | 69.9.3   | MIT |\n" +
        "| pin-project-lite     | 0.2.17   | Apache-2.0 OR MIT |\n" +
        "| pkg-config           | 0.3.33   | MIT OR Apache-2.0 |\n" +
        "| png                  | 0.18.1   | MIT OR Apache-2.0 |\n" +
        "| proc-macro2          | 1.0.106  | MIT OR Apache-2.0 |\n" +
        "| pxfm                 | 0.1.29   | BSD-3-Clause OR Apache-2.0 |\n" +
        "| qt-build-utils       | 0.8.1    | MIT OR Apache-2.0 |\n" +
        "| quick-error          | 2.0.1    | MIT/Apache-2.0 |\n" +
        "| quote                | 1.0.45   | MIT OR Apache-2.0 |\n" +
        "| regex                | 1.12.3   | MIT OR Apache-2.0 |\n" +
        "| regex-automata       | 0.4.14   | MIT OR Apache-2.0 |\n" +
        "| regex-syntax         | 0.8.10   | MIT OR Apache-2.0 |\n" +
        "| rustversion          | 1.0.22   | MIT OR Apache-2.0 |\n" +
        "| same-file            | 1.0.6    | Unlicense/MIT |\n" +
        "| seccompiler          | 0.4.0    | Apache-2.0 OR BSD-3-Clause |\n" +
        "| semver               | 1.0.28   | MIT OR Apache-2.0 |\n" +
        "| serde                | 1.0.228  | MIT OR Apache-2.0 |\n" +
        "| serde_core           | 1.0.228  | MIT OR Apache-2.0 |\n" +
        "| serde_derive         | 1.0.228  | MIT OR Apache-2.0 |\n" +
        "| serde_json           | 1.0.149  | MIT OR Apache-2.0 |\n" +
        "| serde_repr           | 0.1.20   | MIT OR Apache-2.0 |\n" +
        "| sha2                 | 0.10.9   | MIT OR Apache-2.0 |\n" +
        "| sharded-slab         | 0.1.7    | MIT |\n" +
        "| shlex                | 1.3.0    | MIT OR Apache-2.0 |\n" +
        "| simd-adler32         | 0.3.9    | MIT |\n" +
        "| smallvec             | 1.15.1   | MIT OR Apache-2.0 |\n" +
        "| static_assertions    | 1.1.0    | MIT OR Apache-2.0 |\n" +
        "| syn                  | 2.0.117  | MIT OR Apache-2.0 |\n" +
        "| syntect              | 5.3.0    | MIT |\n" +
        "| termcolor            | 1.4.1    | Unlicense OR MIT |\n" +
        "| thiserror            | 1.0.69   | MIT OR Apache-2.0 |\n" +
        "| thiserror            | 2.0.18   | MIT OR Apache-2.0 |\n" +
        "| thiserror-impl       | 1.0.69   | MIT OR Apache-2.0 |\n" +
        "| thiserror-impl       | 2.0.18   | MIT OR Apache-2.0 |\n" +
        "| thread_local         | 1.1.9    | MIT OR Apache-2.0 |\n" +
        "| tiff                 | 0.11.3   | MIT |\n" +
        "| tracing              | 0.1.44   | MIT |\n" +
        "| tracing-attributes   | 0.1.31   | MIT |\n" +
        "| tracing-core         | 0.1.36   | MIT |\n" +
        "| tracing-log          | 0.2.0    | MIT |\n" +
        "| tracing-serde        | 0.2.0    | MIT |\n" +
        "| tracing-subscriber   | 0.3.23   | MIT |\n" +
        "| typenum              | 1.20.1   | MIT OR Apache-2.0 |\n" +
        "| unicode-ident        | 1.0.24   | (MIT OR Apache-2.0) AND Unicode-3.0 |\n" +
        "| unicode-segmentation | 1.13.2   | MIT OR Apache-2.0 |\n" +
        "| unicode-width        | 0.1.14   | MIT OR Apache-2.0 |\n" +
        "| unicode-width        | 0.2.2    | MIT OR Apache-2.0 |\n" +
        "| urlencoding          | 2.1.3    | MIT |\n" +
        "| version_check        | 0.9.5    | MIT/Apache-2.0 |\n" +
        "| walkdir              | 2.5.0    | Unlicense/MIT |\n" +
        "| weezl                | 0.1.12   | MIT OR Apache-2.0 |\n" +
        "| zerocopy             | 0.8.48   | BSD-2-Clause OR Apache-2.0 OR MIT |\n" +
        "| zerocopy-derive      | 0.8.48   | BSD-2-Clause OR Apache-2.0 OR MIT |\n" +
        "| zmij                 | 1.0.21   | MIT |\n" +
        "| zune-core            | 0.5.1    | MIT OR Apache-2.0 OR Zlib |\n" +
        "| zune-inflate         | 0.2.54   | MIT OR Apache-2.0 OR Zlib |\n" +
        "| zune-jpeg            | 0.5.15   | MIT OR Apache-2.0 OR Zlib |\n" +
        // END GENERATED CREDITS TABLE
        "\n" +
        "Where a crate offers multiple licenses, LinSync selects the option\n" +
        "compatible with GPL-3.0-only.  No third-party copyleft Cargo crates\n" +
        "are present in the current dependency tree.\n" +
        "\n" +
        "## License Texts\n" +
        "\n" +
        "### MIT License\n" +
        "\n" +
        "Permission is hereby granted, free of charge, to any person obtaining a\n" +
        "copy of this software and associated documentation files (the\n" +
        "\"Software\"), to deal in the Software without restriction, including\n" +
        "without limitation the rights to use, copy, modify, merge, publish,\n" +
        "distribute, sublicense, and/or sell copies of the Software, and to\n" +
        "permit persons to whom the Software is furnished to do so, subject to\n" +
        "the following conditions:\n" +
        "\n" +
        "The above copyright notice and this permission notice shall be included\n" +
        "in all copies or substantial portions of the Software.\n" +
        "\n" +
        "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND,\n" +
        "EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF\n" +
        "MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.\n" +
        "IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY\n" +
        "CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,\n" +
        "TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE\n" +
        "SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.\n" +
        "\n" +
        "### Apache License 2.0\n" +
        "\n" +
        "Apache License\n" +
        "Version 2.0, January 2004\n" +
        "http://www.apache.org/licenses/\n" +
        "\n" +
        "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION\n" +
        "\n" +
        "1. Definitions.\n" +
        "\n" +
        "\"License\" shall mean the terms and conditions for use, reproduction, and distribution as defined by Sections 1 through 9 of this document.\n" +
        "\n" +
        "\"Licensor\" shall mean the copyright owner or entity authorized by the copyright owner that is granting the License.\n" +
        "\n" +
        "\"Legal Entity\" shall mean the union of the acting entity and all other entities that control, are controlled by, or are under common control with that entity. For the purposes of this definition, \"control\" means (i) the power, direct or indirect, to cause the direction or management of such entity, whether by contract or otherwise, or (ii) ownership of fifty percent (50%) or more of the outstanding shares, or (iii) beneficial ownership of such entity.\n" +
        "\n" +
        "\"You\" (or \"Your\") shall mean an individual or Legal Entity exercising permissions granted by this License.\n" +
        "\n" +
        "\"Source\" form shall mean the preferred form for making modifications, including but not limited to software source code, documentation source, and configuration files.\n" +
        "\n" +
        "\"Object\" form shall mean any form resulting from mechanical transformation or translation of a Source form, including but not limited to compiled object code, generated documentation, and conversions to other media types.\n" +
        "\n" +
        "\"Work\" shall mean the work of authorship, whether in Source or Object form, made available under the License, as indicated by a copyright notice that is included in or attached to the work (an example is provided in the Appendix below).\n" +
        "\n" +
        "\"Derivative Works\" shall mean any work, whether in Source or Object form, that is based on (or derived from) the Work and for which the editorial revisions, annotations, elaborations, or other modifications represent, as a whole, an original work of authorship. For the purposes of this License, Derivative Works shall not include works that remain separable from, or merely link (or bind by name) to the interfaces of, the Work and Derivative Works thereof.\n" +
        "\n" +
        "\"Contribution\" shall mean any work of authorship, including the original version of the Work and any modifications or additions to that Work or Derivative Works thereof, that is intentionally submitted to Licensor for inclusion in the Work by the copyright owner or by an individual or Legal Entity authorized to submit on behalf of the copyright owner. For the purposes of this definition, \"submitted\" means any form of electronic, verbal, or written communication sent to the Licensor or its representatives, including but not limited to communication on electronic mailing lists, source code control systems, and issue tracking systems that are managed by, or on behalf of, the Licensor for the purpose of discussing and improving the Work, but excluding communication that is conspicuously marked or otherwise designated in writing by the copyright owner as \"Not a Contribution.\"\n" +
        "\n" +
        "\"Contributor\" shall mean Licensor and any individual or Legal Entity on behalf of whom a Contribution has been received by Licensor and subsequently incorporated within the Work.\n" +
        "\n" +
        "2. Grant of Copyright License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable copyright license to reproduce, prepare Derivative Works of, publicly display, publicly perform, sublicense, and distribute the Work and such Derivative Works in Source or Object form.\n" +
        "\n" +
        "3. Grant of Patent License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable (except as stated in this section) patent license to make, have made, use, offer to sell, sell, import, and otherwise transfer the Work, where such license applies only to those patent claims licensable by such Contributor that are necessarily infringed by their Contribution(s) alone or by combination of their Contribution(s) with the Work to which such Contribution(s) was submitted. If You institute patent litigation against any entity (including a cross-claim or counterclaim in a lawsuit) alleging that the Work or a Contribution incorporated within the Work constitutes direct or contributory patent infringement, then any patent licenses granted to You under this License for that Work shall terminate as of the date such litigation is filed.\n" +
        "\n" +
        "4. Redistribution. You may reproduce and distribute copies of the Work or Derivative Works thereof in any medium, with or without modifications, and in Source or Object form, provided that You meet the following conditions:\n" +
        "\n" +
        "     (a) You must give any other recipients of the Work or Derivative Works a copy of this License; and\n" +
        "\n" +
        "     (b) You must cause any modified files to carry prominent notices stating that You changed the files; and\n" +
        "\n" +
        "     (c) You must retain, in the Source form of any Derivative Works that You distribute, all copyright, patent, trademark, and attribution notices from the Source form of the Work, excluding those notices that do not pertain to any part of the Derivative Works; and\n" +
        "\n" +
        "     (d) If the Work includes a \"NOTICE\" text file as part of its distribution, then any Derivative Works that You distribute must include a readable copy of the attribution notices contained within such NOTICE file, excluding those notices that do not pertain to any part of the Derivative Works, in at least one of the following places: within a NOTICE text file distributed as part of the Derivative Works; within the Source form or documentation, if provided along with the Derivative Works; or, within a display generated by the Derivative Works, if and wherever such third-party notices normally appear. The contents of the NOTICE file are for informational purposes only and do not modify the License. You may add Your own attribution notices within Derivative Works that You distribute, alongside or as an addendum to the NOTICE text from the Work, provided that such additional attribution notices cannot be construed as modifying the License.\n" +
        "\n" +
        "     You may add Your own copyright statement to Your modifications and may provide additional or different license terms and conditions for use, reproduction, or distribution of Your modifications, or for any such Derivative Works as a whole, provided Your use, reproduction, and distribution of the Work otherwise complies with the conditions stated in this License.\n" +
        "\n" +
        "5. Submission of Contributions. Unless You explicitly state otherwise, any Contribution intentionally submitted for inclusion in the Work by You to the Licensor shall be under the terms and conditions of this License, without any additional terms or conditions. Notwithstanding the above, nothing herein shall supersede or modify the terms of any separate license agreement you may have executed with Licensor regarding such Contributions.\n" +
        "\n" +
        "6. Trademarks. This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except as required for reasonable and customary use in describing the origin of the Work and reproducing the content of the NOTICE file.\n" +
        "\n" +
        "7. Disclaimer of Warranty. Unless required by applicable law or agreed to in writing, Licensor provides the Work (and each Contributor provides its Contributions) on an \"AS IS\" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied, including, without limitation, any warranties or conditions of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A PARTICULAR PURPOSE. You are solely responsible for determining the appropriateness of using or redistributing the Work and assume any risks associated with Your exercise of permissions under this License.\n" +
        "\n" +
        "8. Limitation of Liability. In no event and under no legal theory, whether in tort (including negligence), contract, or otherwise, unless required by applicable law (such as deliberate and grossly negligent acts) or agreed to in writing, shall any Contributor be liable to You for damages, including any direct, indirect, special, incidental, or consequential damages of any character arising as a result of this License or out of the use or inability to use the Work (including but not limited to damages for loss of goodwill, work stoppage, computer failure or malfunction, or any and all other commercial damages or losses), even if such Contributor has been advised of the possibility of such damages.\n" +
        "\n" +
        "9. Accepting Warranty or Additional Liability. While redistributing the Work or Derivative Works thereof, You may choose to offer, and charge a fee for, acceptance of support, warranty, indemnity, or other liability obligations and/or rights consistent with this License. However, in accepting such obligations, You may act only on Your own behalf and on Your sole responsibility, not on behalf of any other Contributor, and only if You agree to indemnify, defend, and hold each Contributor harmless for any liability incurred by, or claims asserted against, such Contributor by reason of your accepting any such warranty or additional liability.\n" +
        "\n" +
        "END OF TERMS AND CONDITIONS\n" +
        "\n" +
        "### BSD 2-Clause License\n" +
        "\n" +
        "Redistribution and use in source and binary forms, with or without\n" +
        "modification, are permitted provided that the following conditions\n" +
        "are met:\n" +
        "\n" +
        " 1. Redistributions of source code must retain the above copyright\n" +
        "    notice, this list of conditions and the following disclaimer.\n" +
        "\n" +
        " 2. Redistributions in binary form must reproduce the above copyright\n" +
        "    notice, this list of conditions and the following disclaimer in\n" +
        "    the documentation and/or other materials provided with the\n" +
        "    distribution.\n" +
        "\n" +
        "THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS\n" +
        "\"AS IS\" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT\n" +
        "LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS\n" +
        "FOR A PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE\n" +
        "COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,\n" +
        "INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES.\n" +
        "\n" +
        "### BSD 3-Clause License\n" +
        "\n" +
        "Redistribution and use in source and binary forms, with or without\n" +
        "modification, are permitted provided that the following conditions\n" +
        "are met:\n" +
        "\n" +
        " 1. Redistributions of source code must retain the above copyright\n" +
        "    notice, this list of conditions and the following disclaimer.\n" +
        "\n" +
        " 2. Redistributions in binary form must reproduce the above copyright\n" +
        "    notice, this list of conditions and the following disclaimer in\n" +
        "    the documentation and/or other materials provided with the\n" +
        "    distribution.\n" +
        "\n" +
        " 3. Neither the name of the copyright holder nor the names of its\n" +
        "    contributors may be used to endorse or promote products derived\n" +
        "    from this software without specific prior written permission.\n" +
        "\n" +
        "THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS\n" +
        "\"AS IS\" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT\n" +
        "LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS\n" +
        "FOR A PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE\n" +
        "COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,\n" +
        "INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES.\n" +
        "\n" +
        "### Zlib License\n" +
        "\n" +
        "This software is provided 'as-is', without any express or implied\n" +
        "warranty.  In no event will the authors be held liable for any\n" +
        "damages arising from the use of this software.\n" +
        "\n" +
        "Permission is granted to anyone to use this software for any\n" +
        "purpose, including commercial applications, and to alter it and\n" +
        "redistribute it freely, subject to the following restrictions:\n" +
        "\n" +
        " 1. The origin of this software must not be misrepresented; you\n" +
        "    must not claim that you wrote the original software.\n" +
        " 2. Altered source versions must be plainly marked as such, and\n" +
        "    must not be misrepresented as being the original software.\n" +
        " 3. This notice may not be removed or altered from any source\n" +
        "    distribution.\n" +
        "\n" +
        "### Unlicense / CC0-1.0 / MIT-0\n" +
        "\n" +
        "These dedications place the work in the public domain (or grant a\n" +
        "no-attribution license equivalent to public domain) world-wide.\n" +
        "See https://unlicense.org, https://creativecommons.org/publicdomain/zero/1.0/,\n" +
        "and the MIT-0 (MIT No Attribution) license text for the canonical\n" +
        "wording.\n" +
        "\n" +
        "### Unicode-3.0 (Unicode License v3)\n" +
        "\n" +
        "Copyright (c) 1991-2024 Unicode, Inc.  All rights reserved.\n" +
        "Distributed under the Terms of Use available at\n" +
        "https://www.unicode.org/copyright.html.  Permission is granted, free\n" +
        "of charge, to any person obtaining a copy of the Unicode data files\n" +
        "and associated documentation, subject to attribution of the Unicode\n" +
        "Consortium and the disclaimer of warranty set out in the license.\n"

    readonly property string acknowledgementsText:
        "# Acknowledgements\n" +
        "\n" +
        "LinSync stands on the shoulders of many open-source communities.\n" +
        "We are grateful to every project, maintainer, and contributor whose\n" +
        "work makes this application possible.\n" +
        "\n" +
        "## Frameworks\n" +
        "\n" +
        " * Qt 6 -- (c) The Qt Company and contributors.\n" +
        "     LGPL-3.0 / GPL-3.0 / commercial.  https://www.qt.io\n" +
        "\n" +
        " * KDE Frameworks 6 -- (c) KDE Community.  LGPL-2.1+.\n" +
        "     https://invent.kde.org/frameworks\n" +
        "\n" +
        " * Kirigami -- KDE's convergent UI toolkit.\n" +
        "     https://invent.kde.org/frameworks/kirigami\n" +
        "\n" +
        " * FreeDesktop.org & XDG portal services -- various authors.\n" +
        "     MIT / LGPL / Apache-2.0.  https://www.freedesktop.org\n" +
        "\n" +
        "## Languages & Toolchain\n" +
        "\n" +
        " * The Rust Project -- https://www.rust-lang.org\n" +
        " * Cargo & the crates.io infrastructure\n" +
        " * The Rust Foundation\n" +
        " * CXX-Qt by KDAB -- https://github.com/KDAB/cxx-qt\n" +
        "\n" +
        "## Visual Design\n" +
        "\n" +
        " * The KDE Visual Design Group -- for the Breeze iconography and\n" +
        "   the Human Interface Guidelines that shape LinSync's chrome.\n" +
        "\n" +
        " * The Material Design and GNOME HIG communities -- for design\n" +
        "   patterns we have learned from over the years.\n" +
        "\n" +
        "## Crate maintainers\n" +
        "\n" +
        "Every crate listed under \"Third-party\" represents the work of one or\n" +
        "more maintainers who chose to release their work as open source.\n" +
        "If your work is included here and you would like a more explicit\n" +
        "acknowledgement, please reach out -- credit is the least we can offer.\n" +
        "\n" +
        "## Inspiration\n" +
        "\n" +
        "LinSync's UX takes cues from prior art in the file-comparison space:\n" +
        "Meld, Kompare, Beyond Compare, KDiff3, and Diffuse.  None of these\n" +
        "projects share code with LinSync -- but each has shaped our thinking\n" +
        "about what a comparison tool can be.\n" +
        "\n" +
        "## You\n" +
        "\n" +
        "Finally: thank you for using LinSync.  Bug reports, feedback, and\n" +
        "pull requests are welcomed at\n" +
        "https://github.com/visorcraft/LinSync.\n"

    function documentTitle(index) {
        switch (index) {
        case 1: return qsTr("Third-party licenses")
        case 2: return qsTr("Acknowledgements")
        default: return qsTr("LinSync License")
        }
    }

    function documentSubtitle(index) {
        switch (index) {
        case 1:
            return qsTr("Every Cargo crate compiled into LinSync, grouped by license expression, plus the full text of every license referenced.")
        case 2:
            return qsTr("Narrative attribution for LinSync's frameworks, toolchain, design influences, and crate maintainers.")
        default:
            return qsTr("The complete GPL-3.0-only license text bundled into the application.")
        }
    }

    function documentBody(index) {
        switch (index) {
        case 1: return page.thirdPartyText
        case 2: return page.acknowledgementsText
        default: return gplLicense.text
        }
    }

    function lineCount(text) {
        if (!text || text.length === 0)
            return 0
        return String(text).split("\n").length
    }

    function lineNumber(value) {
        let s = String(value)
        while (s.length < 5)
            s = " " + s
        return s
    }

    function countMatchingLines(text, query) {
        const needle = String(query).trim().toLowerCase()
        if (needle.length === 0)
            return 0
        const lines = String(text).split("\n")
        let matches = 0
        for (let i = 0; i < lines.length; ++i) {
            if (lines[i].toLowerCase().indexOf(needle) !== -1)
                matches += 1
        }
        return matches
    }

    function filteredBody(text, query) {
        const source = String(text)
        const needle = String(query).trim().toLowerCase()
        if (needle.length === 0)
            return source

        const lines = source.split("\n")
        const matches = []
        for (let i = 0; i < lines.length; ++i) {
            if (lines[i].toLowerCase().indexOf(needle) !== -1)
                matches.push(page.lineNumber(i + 1) + "  " + lines[i])
        }

        if (matches.length === 0)
            return qsTr("No matches for \"%1\".").arg(query)
        return matches.join("\n")
    }

    function setActiveDocument(index) {
        if (page.activeDocument === index)
            return
        page.activeDocument = index
        page.filterText = ""
        filterField.text = ""
    }

    readonly property string currentTitle: page.documentTitle(page.activeDocument)
    readonly property string currentSubtitle: page.documentSubtitle(page.activeDocument)
    readonly property string currentBody: page.documentBody(page.activeDocument)
    readonly property int currentLineCount: page.lineCount(page.currentBody)
    readonly property int matchingLineCount: page.countMatchingLines(page.currentBody, page.filterText)
    readonly property string visibleBody: page.filteredBody(page.currentBody, page.filterText)

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 86
            color: page.themeBgAlt

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: page.separator
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.leftMargin: 24
                anchors.rightMargin: 24
                spacing: 4

                Item { Layout.fillHeight: true }

                Controls.Label {
                    text: qsTr("Licenses")
                    color: page.themeText
                    font.pixelSize: 24
                    font.bold: true
                    font.letterSpacing: 0
                    Layout.fillWidth: true
                }

                Controls.Label {
                    text: qsTr("Bundled license and attribution documents, available without opening a browser.")
                    color: page.themeText
                    font.pixelSize: 12
                    opacity: 0.62
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                }

                Item { Layout.fillHeight: true }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: 24
            Layout.rightMargin: 24
            Layout.topMargin: 16
            Layout.bottomMargin: 24
            spacing: 12

            // Tabs + Copy + Dialog (Dialog only on the LinSync License tab)
            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                Controls.TabBar {
                    id: tabBar
                    Layout.fillWidth: true
                    currentIndex: page.activeDocument
                    onCurrentIndexChanged: page.setActiveDocument(currentIndex)

                    Controls.TabButton {
                        text: qsTr("LinSync License")
                        width: implicitWidth + 16
                    }
                    Controls.TabButton {
                        text: qsTr("Third-party")
                        width: implicitWidth + 16
                    }
                    Controls.TabButton {
                        text: qsTr("Acknowledgements")
                        width: implicitWidth + 16
                    }
                }

                Controls.Button {
                    flat: true
                    text: qsTr("Copy")
                    icon.name: "edit-copy"
                    icon.color: page.themeText
                    display: Controls.AbstractButton.TextBesideIcon
                    onClicked: {
                        documentArea.selectAll()
                        documentArea.copy()
                        documentArea.deselect()
                    }
                    Controls.ToolTip.text: qsTr("Copy the current document to the clipboard")
                    Controls.ToolTip.visible: hovered
                }

                Controls.Button {
                    visible: page.activeDocument === 0
                    flat: true
                    text: qsTr("Dialog")
                    icon.name: "document-preview"
                    icon.color: page.themeText
                    display: Controls.AbstractButton.TextBesideIcon
                    onClicked: licenseDialog.open()
                    Controls.ToolTip.text: qsTr("Open the GPL text in a dialog")
                    Controls.ToolTip.visible: hovered
                }
            }

            // Document title/subtitle on the left, line/match count on the right
            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Controls.Label {
                        text: page.currentTitle
                        color: page.themeText
                        font.pixelSize: 16
                        font.bold: true
                        font.letterSpacing: 0
                        Layout.fillWidth: true
                    }

                    Controls.Label {
                        text: page.currentSubtitle
                        color: page.themeText
                        font.pixelSize: 12
                        opacity: 0.62
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                }

                Controls.Label {
                    text: page.filterText.trim().length > 0
                        ? qsTr("%1 matches").arg(page.matchingLineCount)
                        : qsTr("%1 lines").arg(page.currentLineCount)
                    color: page.themeText
                    font.pixelSize: 11
                    font.family: "monospace"
                    opacity: 0.62
                    Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
                }
            }

            // Filter row: text field + wrap checkbox + clear
            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                AppTextField {
                    id: filterField
                    Layout.fillWidth: true
                    placeholderText: qsTr("Find by crate, package, license, or phrase…")
                    onTextChanged: page.filterText = text
                    Accessible.name: qsTr("Find in license document")
                }

                Controls.CheckBox {
                    text: qsTr("Wrap")
                    checked: page.wrapText
                    onToggled: page.wrapText = checked
                    font.pixelSize: 12
                    palette.windowText: page.themeText
                    palette.text: page.themeText
                }

                Controls.Button {
                    flat: true
                    enabled: page.filterText.length > 0
                    text: qsTr("Clear")
                    icon.name: "edit-clear"
                    icon.color: page.themeText
                    display: Controls.AbstractButton.TextBesideIcon
                    onClicked: {
                        filterField.text = ""
                        page.filterText = ""
                    }
                }
            }

            // Document body
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: 340
                radius: 8
                color: page.themeBgAlt
                border.color: page.separator
                border.width: 1
                clip: true

                Controls.ScrollView {
                    anchors.fill: parent
                    anchors.margins: 12
                    clip: true

                    Controls.TextArea {
                        id: documentArea
                        text: page.visibleBody
                        readOnly: true
                        selectByMouse: true
                        persistentSelection: true
                        wrapMode: page.wrapText ? TextEdit.Wrap : TextEdit.NoWrap
                        textFormat: TextEdit.PlainText
                        color: page.themeText
                        selectedTextColor: page.themeHighlightedText
                        selectionColor: page.themeHighlight
                        font.pixelSize: 12
                        font.family: "monospace"
                        background: Rectangle { color: "transparent" }
                    }
                }
            }
        }
    }

    // GPL license popup ("Dialog" button)
    Controls.Dialog {
        id: licenseDialog
        modal: true
        title: qsTr("GNU General Public License v3")
        standardButtons: Controls.Dialog.Close
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        readonly property int windowWidth: 980
        readonly property int windowHeight: 780

        width: Math.min(licenseDialog.windowWidth - 64, 920)
        height: Math.min(licenseDialog.windowHeight - 64, 680)
        x: Math.max(24, (licenseDialog.windowWidth - width) / 2)
        y: Math.max(24, (licenseDialog.windowHeight - height) / 2)

        palette.window:          page.themeBgAlt
        palette.windowText:      page.themeText
        palette.base:            page.themeBg
        palette.text:            page.themeText
        palette.button:          page.themeBgLift
        palette.buttonText:      page.themeText
        palette.highlight:       page.themeHighlight
        palette.highlightedText: page.themeHighlightedText

        background: Rectangle {
            color: page.themeBgAlt
            radius: 8
            border.color: page.separatorStrong
            border.width: 1
        }

        contentItem: ColumnLayout {
            spacing: 12

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("GPL-3.0-only license text bundled with LinSync.")
                wrapMode: Text.WordWrap
                font.pixelSize: 13
                color: page.themeText
                opacity: 0.7
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: 6
                color: page.themeBg
                border.color: page.separator
                border.width: 1
                clip: true

                Controls.ScrollView {
                    anchors.fill: parent
                    anchors.margins: 12
                    clip: true

                    Controls.TextArea {
                        text: gplLicense.text
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        color: page.themeText
                        selectedTextColor: page.themeHighlightedText
                        selectionColor: page.themeHighlight
                        font.pixelSize: 12
                        font.family: "monospace"
                        background: Rectangle { color: "transparent" }
                    }
                }
            }
        }
    }
}
