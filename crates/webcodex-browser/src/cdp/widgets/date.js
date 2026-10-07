try {
    connected();
    if (disabled(target) || disabled(host)) fail("control_disabled");
    const parts = /^(\d{4})-(\d{2})(?:-(\d{2}))?$/.exec(requested);
    if (!parts) fail("widget_not_supported");
    const year = Number(parts[1]), month = Number(parts[2]), day = parts[3] ? Number(parts[3]) : null;
    const requestedMonth = parts[1] + "-" + parts[2];
    const iso = value => {
        const match = /^(\d{4})[-/.年](\d{1,2})(?:[-/.月](\d{1,2})日?)?月?$/.exec(norm(value));
        return match ? match[1] + "-" + match[2].padStart(2, "0") + (match[3] ? "-" + match[3].padStart(2, "0") : "") : null;
    };
    const complete = () => reading().some(value => iso(value) === requested)
        && attr(target, "aria-invalid") !== "true";
    if (complete()) return { ok: true };

    const inputs = target instanceof view.HTMLInputElement ? [target] : inputControls().filter(visible);
    if (inputs.length === 1 && !inputs[0].readOnly && !inputs[0].disabled
        && attr(inputs[0], "aria-readonly") !== "true" && ["text", "search"].includes(inputs[0].type)) {
        const input = inputs[0], old = input.value, oldState = fieldFingerprint();
        changed = true;
        nativeSet(input, requested);
        await tick();
        await tick();
        if (complete()) return { ok: true };
        // Framework explicitly rejected the edit and restored the original value.
        // Continue inside this operation; never replay a partially accepted edit.
        if (!input.isConnected || input.value !== old || fieldFingerprint() !== oldState) fail("widget_not_supported");
        changed = false;
    }

    await open();
    const roots = () => popupRoots();
    const candidates = () => optionsIn(roots(), true);
    const monthNames = ["january", "february", "march", "april", "may", "june",
        "july", "august", "september", "october", "november", "december"];
    const monthNumber = label => {
        label = norm(label).toLowerCase();
        const numeric = /^0?([1-9]|1[0-2])月?$/.exec(label);
        if (numeric) return Number(numeric[1]);
        const index = monthNames.findIndex(name => label === name || label === name.slice(0, 3));
        return index < 0 ? null : index + 1;
    };
    const controls = () => roots().flatMap(root => walk(root, node =>
        visible(node) && (node instanceof view.HTMLSelectElement || node instanceof view.HTMLInputElement), 32));
    const partControl = name => unique(controls().filter(node => {
        const label = [attr(node, "aria-label"), attr(node, "name"), attr(node, "title"), attr(node, "placeholder")].join(" ").toLowerCase();
        return name === "year" ? /(^|[\s_-])year($|[\s_-])|年份|年\s*$/.test(label)
            : /(^|[\s_-])month($|[\s_-])|月份|月\s*$/.test(label);
    }));
    const assignPart = async (name, requestedPart) => {
        const control = partControl(name);
        if (!control) return false;
        if (control instanceof view.HTMLSelectElement) {
            const options = Array.from(control.options);
            if (options.length > MAX_OPTIONS) fail("widget_bounds_exceeded");
            let matches = options.filter(option => name === "year"
                ? norm(option.text).replace(/年$/, "") === String(requestedPart)
                : monthNumber(option.text) === requestedPart);
            if (!matches.length) {
                const zeroBased = name === "month" && options.some(option => option.value === "0")
                    && !options.some(option => option.value === "12");
                matches = options.filter(option => Number(option.value) === requestedPart - Number(zeroBased));
            }
            const option = unique(matches);
            if (!option) fail("date_not_found");
            if (control.value !== option.value) { nativeSet(control, option.value); await tick(); }
            if (!control.isConnected) return true; // Picker-local rerender; the admitted field remains the same.
            if (control.value !== option.value) fail("date_not_found");
        } else {
            if (control.readOnly) return false;
            if (Number(control.value) !== requestedPart) { nativeSet(control, String(requestedPart)); await tick(); }
            if (control.isConnected && Number(control.value) !== requestedPart) fail("date_not_found");
        }
        return true;
    };
    const fullDateCandidate = () => unique(candidates().filter(node => labels(node).some(value => iso(value) === requested)));
    const clickDate = async node => {
        changed = true;
        activate(node);
        await tick();
        if (!await observe(complete)) fail("date_not_found");
    };
    if (!await observe(() => roots().length > 0)) fail("date_not_found");
    let direct = fullDateCandidate();
    if (direct) { await clickDate(direct); return { ok: true }; }

    const yearSet = await assignPart("year", year);
    const monthSet = await assignPart("month", month);
    direct = fullDateCandidate();
    if (direct) { await clickDate(direct); return { ok: true }; }
    if (!day && await observe(complete, 60)) return { ok: true };

    const headerCandidates = () => roots().flatMap(root => walk(root, node => visible(node)
        && (!node.closest('[role="grid"],[role="gridcell"],[role="row"],tbody')
            || /(?:choose|select|prev(?:ious)?|next).*(?:year|month|decade)/i.test([attr(node, "aria-label"), attr(node, "title")].join(" "))
            || role(node) === "heading" || node.tagName === "CAPTION")
        && !labels(node).some(value => /^\d{4}[-/.]\d{1,2}[-/.]\d{1,2}$/.test(value))
        && (node.tagName === "BUTTON" || role(node) === "heading" || node.tagName === "CAPTION" || node.hasAttribute("aria-live")), 64));
    const headerParts = () => {
        const result = { year: null, month: null };
        for (const node of headerCandidates()) {
            const text = norm(node.textContent).toLowerCase();
            const yearMatch = /(?:^|\D)(\d{4})(?:年|\b)/.exec(text);
            const chinese = /(?:^|\D)(\d{1,2})月/.exec(text);
            const yearMonth = /^(\d{4})[-/.](\d{1,2})$/.exec(text);
            if (yearMatch && !/\d{4}\s*[-~–]\s*\d{4}/.test(text)) result.year = Number(yearMatch[1]);
            if (chinese) result.month = Number(chinese[1]);
            if (yearMonth) { result.year = Number(yearMonth[1]); result.month = Number(yearMonth[2]); }
            const namedMonth = monthNames.findIndex(name => text.includes(name));
            if (namedMonth >= 0) result.month = namedMonth + 1;
        }
        return result;
    };
    const chooseYear = async () => {
        if (yearSet || headerParts().year === year) return true;
        const header = unique(headerCandidates().filter(node => node.tagName === "BUTTON"
            && /^\d{4}年?$/.test(norm(node.textContent))));
        if (!header) return false;
        activate(header);
        await tick();
        for (let page = 0; page < 12; page++) {
            const list = await observe(() => {
                const available = candidates().filter(node => /^\d{4}年?$/.test(norm(node.textContent)));
                return available.length > 1 ? available : null;
            }, 180);
            if (!list) return false;
            const wanted = unique(list.filter(node => Number(norm(node.textContent).replace(/年$/, "")) === year));
            if (wanted) { activate(wanted); await tick(); return true; }
            const years = list.map(node => Number(norm(node.textContent).replace(/年$/, "")));
            const direction = year < Math.min(...years) ? "previous" : year > Math.max(...years) ? "next" : null;
            if (!direction) return false;
            const navigation = unique(headerCandidates().filter(node => {
                const label = [attr(node, "aria-label"), attr(node, "title"), node.textContent].join(" ").toLowerCase();
                return direction === "previous" ? /\bprev(?:ious)?\b.*(?:year|decade)|上(?:一)?(?:年|十年)|前(?:一)?(?:年|十年)/.test(label)
                    : /\bnext\b.*(?:year|decade)|下(?:一)?(?:年|十年)|后(?:一)?(?:年|十年)/.test(label);
            }));
            if (!navigation) return false;
            const previousYears = years.join(",");
            activate(navigation);
            await tick();
            if (!await observe(() => candidates().filter(node => /^\d{4}年?$/.test(norm(node.textContent)))
                .map(node => Number(norm(node.textContent).replace(/年$/, ""))).join(",") !== previousYears, 250)) return false;
        }
        return false;
    };
    const yearReady = await chooseYear();
    const chooseMonth = async () => {
        if (monthSet || headerParts().month === month) return true;
        const explicitMonth = node => {
            const text = norm(node.textContent).toLowerCase();
            return /月$/.test(text) || monthNames.some(name => text === name || text === name.slice(0, 3));
        };
        const monthHeader = () => unique(headerCandidates().filter(node => {
            if (node.tagName !== "BUTTON" || monthNumber(norm(node.textContent)) === null) return false;
            return explicitMonth(node) || /month|月份/i.test([attr(node, "aria-label"), attr(node, "title")].join(" "));
        }));
        // Year selection may redraw its header/panel asynchronously.
        const opportunity = await observe(() => {
            const exact = unique(candidates().filter(node => labels(node).some(value => iso(value) === requestedMonth)));
            if (exact) return { option: exact };
            const header = monthHeader();
            if (header) return { header };
            const option = unique(candidates().filter(node => explicitMonth(node) && monthNumber(norm(node.textContent)) === month));
            return option ? { option } : null;
        });
        if (!opportunity) return false;
        let wanted = opportunity.option;
        if (opportunity.header) {
            const previous = new Map(candidates().map(node => [node, labels(node).join("|")]));
            activate(opportunity.header);
            await tick();
            wanted = await observe(() => unique(candidates().filter(node => {
                if (labels(node).some(value => iso(value) === requestedMonth)) return true;
                if (monthNumber(norm(node.textContent)) !== month) return false;
                const scope = node.closest('[role="grid"],[role="listbox"]');
                const monthScope = scope && /month|月份/i.test(attr(scope, "aria-label"));
                // Opening a header alone is not proof that numeric day cells became months.
                return explicitMonth(node) || monthScope || !previous.has(node) || previous.get(node) !== labels(node).join("|");
            })));
        }
        if (!wanted) return false;
        if (!day) changed = true;
        activate(wanted);
        await tick();
        return true;
    };
    const monthReady = await chooseMonth();
    if (!day) {
        if (!await observe(complete)) fail("date_not_found");
        return { ok: true };
    }
    direct = await observe(() => {
        const exact = fullDateCandidate();
        if (exact) return exact;
        const header = headerParts();
        if (!yearReady || !monthReady || header.year !== year || header.month !== month) return null;
        return unique(candidates().filter(node => norm(node.textContent) === String(day)
            && !/\b(?:outside|other-month)\b/.test(attr(node, "class"))));
    });
    if (!direct) fail("date_not_found");
    await clickDate(direct);
    return { ok: true };
} catch (error) {
    return await rejected(error);
}
