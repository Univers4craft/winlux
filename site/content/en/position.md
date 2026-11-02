---
title: Legal position
description: Why Winlux respects software and trademark law, what it commits to, and how to reach the project if you hold rights.
---

*Statement of 2 November 2026, addressed to Microsoft Corporation and to any rights holder.*

## Who we are

Winlux is **free software** (GNU GPL version 3 or later), developed as a **community, non-commercial** project: a system on the Linux kernel, with its own compatibility layer, that opens programs made for Windows.

Winlux **is not Microsoft**, is **neither affiliated with nor endorsed by Microsoft**, and claims no connection with it. It **does not aim to replace Windows** or to present itself as Windows: it lets people who use Linux open the programs they already own. Windows is a trademark of Microsoft Corporation; we only name it to say which programs Winlux is compatible with.

## The name Winlux

"Winlux" is made of two common words:

- **win**, an everyday English word meaning to succeed or come out on top. A common word on its own belongs to everyone: many programs have names starting with these three letters and coexist with the Windows trademark, for example WinZip and WinDVD, registered trademarks of Corel;
- **lux**, a Latin word meaning "light", and the unit of illuminance of the International System of Units (symbol lx). This is the meaning carried by the project's identity: its logo is called Prisme (prism), the object that splits light, and its wallpapers are called Night and Dawn.

The name therefore reads as "the light that lets the user win". It contains neither the word Windows nor the word Linux, nor the element that distinguishes them: it does not take "dows".

Likelihood of confusion is assessed globally, from the overall impression the signs give visually, aurally and conceptually, taking account of their distinctive and dominant components (**Court of Justice, 11 November 1997, Sabel, C-251/95**). Visually, Winlux and Windows share only their first three letters out of seven. Aurally, "win-lux" ends on a short, sharp sound that does not rhyme with Windows. Conceptually, light and victory have nothing to do with windows.

What we openly assume is compatibility: Winlux opens programs made for Windows, we say so to indicate the intended purpose of our software (section 3), and that compatibility is lawful as interoperability (sections 1 and 2). The name borrows no protected sign to say it.

## 1. Interoperability

- **Directive 2009/24/EC, Article 1(2)**: ideas and principles underlying any element of a program, including its interfaces, are not protected by copyright.
- **Directive 2009/24/EC, Article 5(3)**, and **French Intellectual Property Code, Article L. 122-6-1, III**: a lawful user may observe, study and test how a program works to determine its underlying ideas and principles.
- **Directive 2009/24/EC, Article 6**, and **Article L. 122-6-1, IV**: reproducing and translating code is allowed when indispensable to achieve the interoperability of an independently created program, by a person entitled to use the program, for information not already readily available, and limited to the parts needed.
- **Directive 2009/24/EC, Article 8**, and **Article L. 122-6-1, V**: contractual terms contrary to these exceptions are void.

What we do: write our own functions that reproduce the documented behaviour of Windows functions. What we do not do: copy Microsoft code (**Directive, Article 4**; **Article L. 122-6**), use information for anything other than interoperability, or distribute means to defeat the protection of a program (**Directive, Article 7(1)(c)**; **Article L. 122-6-2**).

## 2. Programming interfaces

- **Court of Justice of the European Union, 2 May 2012, SAS Institute v World Programming, C-406/10**: the functionality of a program, its programming language and its data file formats are not protected by software copyright.
- **Supreme Court of the United States, 5 April 2021, Google v. Oracle, 593 U.S. 1**: reusing the declarations of a programming interface to allow interoperability was held to be fair use.

Winlux reproduces the behaviour of documented functions; it does not take Microsoft's code, documentation or header files.

## 3. Our name and the Windows trademark

- **Regulation (EU) 2017/1001, Article 9(2)**, and **French Intellectual Property Code, Articles L. 713-2 and L. 713-3**: a trademark protects against a similar sign that creates a likelihood of confusion and, for a mark with a reputation, against use that takes unfair advantage of that reputation.
- **Regulation (EU) 2017/1001, Article 14(1)(c) and 14(2)**, and **Article L. 713-6, I, 3°**: a trademark may be named to indicate the intended purpose of a product, in line with honest practices.
- **Court of Justice, 17 March 2005, Gillette, C-228/03**: such use is honest if it does not suggest a commercial connection, does not discredit the mark and does not present the product as an imitation.

The name Winlux does not take the word Windows and does not look or sound like it beyond three letters, a prefix shared by many programs. We name Windows only to indicate compatibility, and we do not present ourselves as Windows, as an edition or replacement of Windows, or as a Microsoft product.

## 4. Fair competition

**French Civil Code, Article 1240**: whoever causes harm to another through their fault must repair it; this is the basis of unfair competition and free-riding claims. To never ride on Microsoft's efforts: our own identity (Prisme logo, wallpapers, sounds and icons made by the project), no sales, no comparison with Windows in our communication, and the non-affiliation notice on every page of the website and in the system's About screen.

**Browser choice.** In the spirit of the European rules on browser choice, Winlux neither ships nor favours any browser: the installation image contains none, none is selected by default, and users pick their own from an alphabetical list (Settings > Browsers, installer, first start). Winlux then downloads it from its publisher's official website, checks the file and installs it; users can change it at any time. Those rules target large systems: **Regulation (EU) 2022/1925 on digital markets (Digital Markets Act), Article 6(3)**, requires gatekeepers to let users easily uninstall applications and change default settings, with a choice screen; before it, the European Commission made binding Microsoft's commitments to offer a browser choice screen (**case COMP/39.530, decision of 16 December 2009**). Winlux is not a gatekeeper and is not bound by these rules; it follows the principle of its own accord. Browsers appear under their real name, that of their publisher.

## 5. Commitments anyone can check

Everything can be checked in the [public repository](https://github.com/Univers4craft/winlux):

1. **Free licence**: code under GNU GPL version 3 or later; third-party components and the written source code offer in the [NOTICES](https://github.com/Univers4craft/winlux/blob/master/NOTICES) file.
2. **No Microsoft material**: no Microsoft code, library, executable, font, icon, sound, wallpaper or logo in the project's code and resources (its public repository) or in the installation image, which an automatic check verifies at every build. Programs from other publishers that users choose to install, such as a web browser, remain theirs, under their own licence; they appear under their real name, which refers to the product of its publisher (**Regulation (EU) 2017/1001, Article 14(1)(c)**).
3. **Clean room**: the [contributor guide](https://github.com/Univers4craft/winlux/blob/master/AGENTS.md) forbids reading or copying Microsoft source code and decompiling a Microsoft library to write code.
4. **Reproducible test files**: Windows-format test files are built by the project, with free tools, from sources in the repository.
5. **No replacement claims**: the project does not present itself as an alternative to or replacement for Windows.
6. **No circumvention**: nothing to defeat a technical protection, an activation or a licence.
7. **Non-commercial**: Winlux is free of charge.

## 6. If you believe something infringes your rights

Open a request on [the project's issues page](https://github.com/Univers4craft/winlux/issues) titled "Request from a rights holder", naming the item concerned and the right invoked. We acknowledge receipt as soon as we can, review the request in good faith, remove or change the item if the request is founded, and answer you in every case. This process is neither an admission of infringement nor a waiver of any right.

## Notice

This page states **the position of the Winlux project**. **It is not legal advice.** It should be reviewed by a trademark attorney or a lawyer. References and their sources are in the [repository document](https://github.com/Univers4craft/winlux/blob/master/docs/POSITION-JURIDIQUE.md).
