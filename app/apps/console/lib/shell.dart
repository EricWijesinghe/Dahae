// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:ui/ui.dart';

import 'console_title.dart';
import 'nav_sections.dart';
import 'screens/bookings.dart';
import 'server_moved_banner.dart';
import 'state.dart';

/// Linear-style: side menu on desktop, drawer on phone, Ctrl+K palette, Ctrl+1..9 to jump.
class ConsoleShell extends StatefulWidget {
  const ConsoleShell({super.key});
  @override
  State<ConsoleShell> createState() => _ConsoleShellState();
}

class _ConsoleShellState extends State<ConsoleShell> {
  int _index = 0;
  final _pages = <int, Widget>{};

  @override
  void initState() {
    super.initState();
    final s = context.consoleRead;
    s.loadDashboard().ignore();
    s.realtime.connect();
  }

  List<ConsoleSection> get _visible => consoleSections
      .where((s) => !s.adminOnly || context.console.isAdmin)
      .toList();

  void _go(int i) => setState(() => _index = i);

  Future<void> _palette() async {
    final t = context.t;
    final picked = await showDialog<Object>(
      context: context,
      builder: (_) => _Palette(sections: _visible, t: t),
    );
    if (picked is int) _go(picked);
    if (picked is String && mounted) {
      _go(1);
      await Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => BookingDetailPage(query: picked),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final c = context.console;
    final sections = _visible;
    final page = ServerMovedBanner(
      child: _pages.putIfAbsent(_index, () => sections[_index].build()),
    );
    final shortcuts = <ShortcutActivator, VoidCallback>{
      const SingleActivator(LogicalKeyboardKey.keyK, control: true): _palette,
      const SingleActivator(LogicalKeyboardKey.keyK, meta: true): _palette,
      for (var i = 0; i < sections.length && i < 9; i++)
        SingleActivator(
          LogicalKeyboardKey(0x00000000031 + i),
          control: true,
        ): () =>
            _go(i),
    };
    Widget navItem(int i) {
      final s = sections[i];
      final badge = switch (i) {
        2 => c.openQuotes,
        3 => c.unreadThreads,
        _ => 0,
      };
      final selected = i == _index;
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: Space.xs, vertical: 1),
        child: ListTile(
          dense: true,
          selected: selected,
          selectedTileColor: context.colors.primaryTint,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
          ),
          // Dense desktop rail: no tilt or bounce, the selected row shows by colour only.
          leading: MediaQuery(
            data: MediaQuery.of(context).copyWith(disableAnimations: true),
            child: ExcludeSemantics(
              child: Icon3D(s.id, active: selected, size: 24),
            ),
          ),
          title: KeepAllText(
            s.label(t),
            style: context.text.labelLarge?.copyWith(
              fontSize: 14,
              fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
            ),
          ),
          trailing: badge > 0 ? Badge(label: Text('$badge')) : null,
          onTap: () {
            _go(i);
            if (!context.isWide) Navigator.of(context).maybePop();
          },
        ),
      );
    }

    final menu = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            Space.md,
            Space.lg,
            Space.md,
            Space.sm,
          ),
          child: Row(
            children: [
              const DahaeLogo(size: 26),
              const SizedBox(width: Space.xs),
              Expanded(
                child: KeepAllText(
                  consoleTitle(context),
                  style: context.text.titleSmall,
                ),
              ),
              const _ThemeButton(),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: Space.sm,
            vertical: Space.xs,
          ),
          child: OutlinedButton.icon(
            style: OutlinedButton.styleFrom(
              minimumSize: const Size.fromHeight(40),
              alignment: Alignment.centerLeft,
            ),
            icon: const Icon(Icons.search_rounded, size: 18),
            label: Row(
              children: [
                Expanded(
                  child: KeepAllText(t.cSearchCommand, style: context.text.bodySmall),
                ),
                KeepAllText('Ctrl K', style: context.text.labelSmall),
              ],
            ),
            onPressed: _palette,
          ),
        ),
        Expanded(
          child: ListView(
            children: [for (var i = 0; i < sections.length; i++) navItem(i)],
          ),
        ),
        const Divider(),
        ListTile(
          dense: true,
          leading: const Icon(Icons.article_rounded, size: 20),
          title: KeepAllText(t.moreLicenses),
          onTap: () =>
              showDahaeLicensePage(context, applicationName: context.brandName),
        ),
        ListTile(
          dense: true,
          leading: const Icon(Icons.logout_rounded, size: 20),
          title: KeepAllText(t.logout),
          onTap: Deps.read(context).session.logout,
        ),
        Padding(
          padding: const EdgeInsets.all(Space.sm),
          child: KeepAllText(
            t.legalCopyright,
            style: context.text.labelSmall,
            textAlign: TextAlign.center,
          ),
        ),
      ],
    );

    return CallbackShortcuts(
      bindings: shortcuts,
      child: Focus(
        autofocus: true,
        child: context.isWide
            ? Scaffold(
                body: Row(
                  children: [
                    SizedBox(
                      width: 248,
                      child: AuroraBackground(
                        child: GlassBar(
                          horizontal: false,
                          child: Material(
                            color: Colors.transparent,
                            child: menu,
                          ),
                        ),
                      ),
                    ),
                    const VerticalDivider(width: 1),
                    Expanded(child: page),
                  ],
                ),
              )
            : Scaffold(
                appBar: AppBar(
                  title: KeepAllText(sections[_index].label(t)),
                  actions: [
                    IconButton(
                      tooltip: t.cSearchCommand,
                      onPressed: _palette,
                      icon: const Icon(Icons.search_rounded),
                    ),
                  ],
                ),
                drawer: Drawer(child: SafeArea(child: menu)),
                body: page,
              ),
      ),
    );
  }
}

/// Opens the three-option theme sheet (shared with the customer app). Icon shows the current mode.
class _ThemeButton extends StatelessWidget {
  const _ThemeButton();

  @override
  Widget build(BuildContext context) {
    final theme = Deps.of(context).theme;
    final icon = switch (theme.mode) {
      ThemeMode.dark => Icons.dark_mode_rounded,
      ThemeMode.light => Icons.light_mode_rounded,
      ThemeMode.system => Icons.brightness_auto_rounded,
    };
    return IconButton(
      tooltip: '${context.t.themeTip}: ${context.t.themeLabel(theme.mode)}',
      visualDensity: VisualDensity.compact,
      onPressed: () => showThemeSheet(context, theme),
      icon: Icon(icon, size: 20),
    );
  }
}

class _Palette extends StatefulWidget {
  const _Palette({required this.sections, required this.t});
  final List<ConsoleSection> sections;
  final T t;
  @override
  State<_Palette> createState() => _PaletteState();
}

class _PaletteState extends State<_Palette> {
  String _q = '';

  @override
  Widget build(BuildContext context) {
    final matches = [
      for (var i = 0; i < widget.sections.length; i++)
        if (widget.sections[i]
            .label(widget.t)
            .toLowerCase()
            .contains(_q.toLowerCase()))
          i,
    ];
    final looksLikeBooking = _q.trim().length >= 3 && matches.isEmpty;
    return Dialog(
      alignment: const Alignment(0, -0.6),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560, maxHeight: 440),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.all(Space.md),
              child: TextField(
                autofocus: true,
                decoration: InputDecoration(
                  prefixIcon: const Icon(Icons.search_rounded),
                  hintText: widget.t.cPaletteHint,
                ),
                onChanged: (v) => setState(() => _q = v),
                onSubmitted: (_) => Navigator.of(context).pop(
                  matches.isNotEmpty
                      ? matches.first
                      : (_q.trim().isEmpty ? null : _q.trim()),
                ),
              ),
            ),
            Flexible(
              child: ListView(
                shrinkWrap: true,
                children: [
                  for (final i in matches)
                    ListTile(
                      leading: Icon(widget.sections[i].icon),
                      title: KeepAllText(widget.sections[i].label(widget.t)),
                      trailing: i < 9 ? KeepAllText('Ctrl ${i + 1}') : null,
                      onTap: () => Navigator.of(context).pop(i),
                    ),
                  if (looksLikeBooking)
                    ListTile(
                      leading: const Icon(Icons.tag_rounded),
                      title: KeepAllText('${widget.t.cOpenBooking}: ${_q.trim()}'),
                      onTap: () => Navigator.of(context).pop(_q.trim()),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
