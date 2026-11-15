# REA Extraction Evidence Index (Clean-Room JSON Records)

This directory stores structured reverse-engineering outputs extracted via **REA** (`project_android_application_graph`, JADX headless, and ELF native inspection providers) from `Clash+of+Clans_18.647.5_APKPure.apk` solely as architectural & rendering references for `CampusLingo: Village Clash`.

> **Clean-Room Policy:** Raw third-party `.apk`, `.so` (`libapp.so`, `libg.so`, `libscid_sdk.so`, `libsupercell_clashofclans.so`), `.glb`, `.sctx`, `.sc`, `.sfx`, and `.ttf` binaries are **strictly excluded** from this repository.

## Primary Combined Exports
- [`coc_apk_evidence.json`](./coc_apk_evidence.json) — REA safe artifact graph provider export (`APK` structure, shaders, `assets/sc3d/`, `assets/sfx/`, `assets/titan/` inventory)
- [`coc_jadx_evidence.json`](./coc_jadx_evidence.json) — JADX headless Android component & class analysis JSON
- [`coc_evidence_bundle.json`](./coc_evidence_bundle.json) — Exported REA investigation evidence bundle

## Deduplicated Per-Operation REA Step Records

| Step | REA Operation | Subject | Size (KB) | File |
| ---: | :--- | :--- | ---: | :--- |
| `1128` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `3.6` | [`step_1128_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1128_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1140` | `project_android_application_graph` | `Clash+of+Clans_18.647.5_APKPure.apk` | `26.4` | [`step_1140_project_android_application_graph_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1140_project_android_application_graph_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1159` | `extract_artifact` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.2` | [`step_1159_extract_artifact_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1159_extract_artifact_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1160` | `identify_flutter_build` | `Clash+of+Clans_18.647.5_APKPure.apk` | `5.9` | [`step_1160_identify_flutter_build_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1160_identify_flutter_build_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1162` | `inspect_binary_hardening` | `libg.so` | `4.8` | [`step_1162_inspect_binary_hardening_libg.so.json`](steps/step_1162_inspect_binary_hardening_libg.so.json) |
| `1163` | `inspect_elf_notes` | `libg.so` | `3.1` | [`step_1163_inspect_elf_notes_libg.so.json`](steps/step_1163_inspect_elf_notes_libg.so.json) |
| `1164` | `inspect_analysis_view` | `libg.so` | `3.2` | [`step_1164_inspect_analysis_view_libg.so.json`](steps/step_1164_inspect_analysis_view_libg.so.json) |
| `1165` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `3.3` | [`step_1165_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1165_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1166` | `inspect_elf_notes` | `libscid_sdk.so` | `4.3` | [`step_1166_inspect_elf_notes_libscid_sdk.so.json`](steps/step_1166_inspect_elf_notes_libscid_sdk.so.json) |
| `1170` | `inspect_rust_binary` | `libsupercell_clashofclans.so` | `3.6` | [`step_1170_inspect_rust_binary_libsupercell_clashofclans.so.json`](steps/step_1170_inspect_rust_binary_libsupercell_clashofclans.so.json) |
| `1171` | `inspect_rust_binary` | `libscid_sdk.so` | `3.5` | [`step_1171_inspect_rust_binary_libscid_sdk.so.json`](steps/step_1171_inspect_rust_binary_libscid_sdk.so.json) |
| `1172` | `inspect_analysis_view` | `libscid_sdk.so` | `3.0` | [`step_1172_inspect_analysis_view_libscid_sdk.so.json`](steps/step_1172_inspect_analysis_view_libscid_sdk.so.json) |
| `1175` | `inspect_analysis_view` | `libscid_sdk.so` | `8.3` | [`step_1175_inspect_analysis_view_libscid_sdk.so.json`](steps/step_1175_inspect_analysis_view_libscid_sdk.so.json) |
| `1176` | `inspect_dart_aot` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.8` | [`step_1176_inspect_dart_aot_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1176_inspect_dart_aot_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1275` | `evidence_bundle` | `bundle` | `75.5` | [`coc_evidence_bundle.json`](./coc_evidence_bundle.json) |
| `1455` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `42.7` | [`step_1455_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1455_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1456` | `decode_android_resources` | `Clash+of+Clans_18.647.5_APKPure.apk` | `34.9` | [`step_1456_decode_android_resources_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1456_decode_android_resources_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1461` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `139.5` | [`step_1461_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1461_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1462` | `search_android_classes` | `Clash+of+Clans_18.647.5_APKPure.apk` | `7.0` | [`step_1462_search_android_classes_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1462_search_android_classes_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1463` | `search_android_classes` | `Clash+of+Clans_18.647.5_APKPure.apk` | `3.3` | [`step_1463_search_android_classes_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1463_search_android_classes_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1468` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `41.8` | [`step_1468_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1468_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1469` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `9.0` | [`step_1469_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1469_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1476` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.9` | [`step_1476_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1476_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1478` | `project_android_application_graph` | `Clash+of+Clans_18.647.5_APKPure.apk` | `26.4` | [`step_1478_project_android_application_graph_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1478_project_android_application_graph_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1479` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `5.7` | [`step_1479_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1479_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1480` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `48.6` | [`step_1480_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1480_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1481` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `6.5` | [`step_1481_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1481_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1487` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.8` | [`step_1487_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1487_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1488` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.6` | [`step_1488_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1488_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1489` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.8` | [`step_1489_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1489_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1492` | `extract_artifact` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.6` | [`step_1492_extract_artifact_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1492_extract_artifact_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1493` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.2` | [`step_1493_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1493_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1494` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `6.9` | [`step_1494_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1494_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1499` | `inspect_binary_hardening` | `libg.so` | `4.8` | [`step_1499_inspect_binary_hardening_libg.so.json`](steps/step_1499_inspect_binary_hardening_libg.so.json) |
| `1501` | `inspect_analysis_view` | `libg.so` | `3.2` | [`step_1501_inspect_analysis_view_libg.so.json`](steps/step_1501_inspect_analysis_view_libg.so.json) |
| `1502` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `3.3` | [`step_1502_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1502_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1503` | `inspect_native_initializers` | `libg.so` | `23.2` | [`step_1503_inspect_native_initializers_libg.so.json`](steps/step_1503_inspect_native_initializers_libg.so.json) |
| `1504` | `inspect_native_initializers` | `libsupercell_clashofclans.so` | `4.8` | [`step_1504_inspect_native_initializers_libsupercell_clashofclans.so.json`](steps/step_1504_inspect_native_initializers_libsupercell_clashofclans.so.json) |
| `1505` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `7.3` | [`step_1505_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1505_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1506` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `4.4` | [`step_1506_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1506_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1513` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `7.4` | [`step_1513_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1513_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1514` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `7.9` | [`step_1514_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1514_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1522` | `binary_overview` | `libsupercell_clashofclans.so` | `7.0` | [`step_1522_binary_overview_libsupercell_clashofclans.so.json`](steps/step_1522_binary_overview_libsupercell_clashofclans.so.json) |
| `1523` | `list_segments` | `libsupercell_clashofclans.so` | `8.5` | [`step_1523_list_segments_libsupercell_clashofclans.so.json`](steps/step_1523_list_segments_libsupercell_clashofclans.so.json) |
| `1524` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `5.3` | [`step_1524_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1524_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1525` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `5.3` | [`step_1525_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1525_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1526` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `5.3` | [`step_1526_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1526_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1527` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `5.3` | [`step_1527_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1527_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1531` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `4.4` | [`step_1531_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1531_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1540` | `list_procedures` | `libsupercell_clashofclans.so` | `77.8` | [`step_1540_list_procedures_libsupercell_clashofclans.so.json`](steps/step_1540_list_procedures_libsupercell_clashofclans.so.json) |
| `1541` | `list_strings` | `libsupercell_clashofclans.so` | `95.1` | [`step_1541_list_strings_libsupercell_clashofclans.so.json`](steps/step_1541_list_strings_libsupercell_clashofclans.so.json) |
| `1547` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `3.3` | [`step_1547_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1547_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1552` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `12.3` | [`step_1552_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1552_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1553` | `read_bytes` | `libsupercell_clashofclans.so` | `3.9` | [`step_1553_read_bytes_libsupercell_clashofclans.so.json`](steps/step_1553_read_bytes_libsupercell_clashofclans.so.json) |
| `1554` | `read_bytes` | `libsupercell_clashofclans.so` | `3.9` | [`step_1554_read_bytes_libsupercell_clashofclans.so.json`](steps/step_1554_read_bytes_libsupercell_clashofclans.so.json) |
| `1559` | `procedure_pseudo_code` | `libsupercell_clashofclans.so` | `6.2` | [`step_1559_procedure_pseudo_code_libsupercell_clashofclans.so.json`](steps/step_1559_procedure_pseudo_code_libsupercell_clashofclans.so.json) |
| `1560` | `inspect_analysis_view` | `libsupercell_clashofclans.so` | `5.3` | [`step_1560_inspect_analysis_view_libsupercell_clashofclans.so.json`](steps/step_1560_inspect_analysis_view_libsupercell_clashofclans.so.json) |
| `1564` | `inspect_analysis_view` | `libg.so` | `3.5` | [`step_1564_inspect_analysis_view_libg.so.json`](steps/step_1564_inspect_analysis_view_libg.so.json) |
| `1565` | `inspect_analysis_view` | `Clash+of+Clans_18.647.5_APKPure.apk` | `5.8` | [`step_1565_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1565_inspect_analysis_view_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1568` | `inspect_dart_aot` | `Clash+of+Clans_18.647.5_APKPure.apk` | `3.7` | [`step_1568_inspect_dart_aot_Clash_of_Clans_18.647.5_APKPure.apk.json`](steps/step_1568_inspect_dart_aot_Clash_of_Clans_18.647.5_APKPure.apk.json) |
| `1570` | `binary_overview` | `libg.so` | `6.6` | [`step_1570_binary_overview_libg.so.json`](steps/step_1570_binary_overview_libg.so.json) |
| `1572` | `list_segments` | `libg.so` | `7.3` | [`step_1572_list_segments_libg.so.json`](steps/step_1572_list_segments_libg.so.json) |
| `1573` | `list_procedures` | `libg.so` | `395.6` | [`step_1573_list_procedures_libg.so.json`](steps/step_1573_list_procedures_libg.so.json) |
| `1574` | `list_strings` | `libg.so` | `248.7` | [`step_1574_list_strings_libg.so.json`](steps/step_1574_list_strings_libg.so.json) |
| `1575` | `inspect_analysis_view` | `libg.so` | `5.2` | [`step_1575_inspect_analysis_view_libg.so.json`](steps/step_1575_inspect_analysis_view_libg.so.json) |
| `1580` | `procedure_pseudo_code` | `libg.so` | `4.0` | [`step_1580_procedure_pseudo_code_libg.so.json`](steps/step_1580_procedure_pseudo_code_libg.so.json) |
| `1581` | `inspect_analysis_view` | `libg.so` | `12.2` | [`step_1581_inspect_analysis_view_libg.so.json`](steps/step_1581_inspect_analysis_view_libg.so.json) |
| `1583` | `procedure_pseudo_code` | `libg.so` | `4.0` | [`step_1583_procedure_pseudo_code_libg.so.json`](steps/step_1583_procedure_pseudo_code_libg.so.json) |
| `1584` | `read_bytes` | `libg.so` | `3.9` | [`step_1584_read_bytes_libg.so.json`](steps/step_1584_read_bytes_libg.so.json) |
| `1585` | `read_bytes` | `libg.so` | `3.9` | [`step_1585_read_bytes_libg.so.json`](steps/step_1585_read_bytes_libg.so.json) |
| `1586` | `read_bytes` | `libg.so` | `3.9` | [`step_1586_read_bytes_libg.so.json`](steps/step_1586_read_bytes_libg.so.json) |
| `1589` | `procedure_callees` | `libg.so` | `4.0` | [`step_1589_procedure_callees_libg.so.json`](steps/step_1589_procedure_callees_libg.so.json) |
| `1592` | `procedure_callees` | `libg.so` | `4.0` | [`step_1592_procedure_callees_libg.so.json`](steps/step_1592_procedure_callees_libg.so.json) |
| `1598` | `inspect_binary_hardening` | `libscid_sdk.so` | `4.9` | [`step_1598_inspect_binary_hardening_libscid_sdk.so.json`](steps/step_1598_inspect_binary_hardening_libscid_sdk.so.json) |
| `1599` | `inspect_native_exports` | `libscid_sdk.so` | `1508.9` | [`step_1599_inspect_native_exports_libscid_sdk.so.json`](steps/step_1599_inspect_native_exports_libscid_sdk.so.json) |
