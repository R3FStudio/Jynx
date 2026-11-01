# Jynx Privacy Policy

Effective date: November 1, 2026

## Overview

Jynx is developed by R3F Studio. This policy explains the privacy behavior of the Jynx CLI, agent harness, SDKs, and official clients distributed by R3F Studio. It covers Jynx software and R3F Studio's project channels; separately operated applications, model providers, websites, and integrations have their own privacy practices.

R3F Studio does not operate a built-in Jynx account service or a central service that receives your conversations, project files, model inputs, or generated answers. Official Jynx software does not include built-in advertising trackers, usage analytics, or an automatic crash-report upload service. R3F Studio does not receive your local conversations merely because you install or use Jynx.

Local inference does not mean every feature is offline. Update checks, downloads, remote model APIs, searches, websites, and integrations can make network requests. The sections below explain those paths and the choices available to you.

## Information processed on your device

Depending on the features you use, Jynx may process:

- Messages, generated answers, conversation summaries, tool activity, and session state.
- Files, source code, directory names, terminal commands, command output, images, and attachments you provide or allow tools to access.
- Model selections, inference settings, permissions, preferences, model download records, and integration configuration.
- API keys or other credentials you configure for an external service.
- Local application diagnostics, errors, and runtime activity used to display status or troubleshoot failures.

An agent can inspect more than the text you type. If you authorize a file read, command, browser action, or integration, its results can enter the model's context. Context compaction summarizes information for continued use; it does not necessarily remove all earlier information from local storage or from a provider that already received it.

Local state is stored in the locations used by the client or selected in configuration. The CLI and harness normally use a state directory on the machine running them; `JYNX_STATE_DIR` can select another location. Graphical clients may also keep preferences, credentials, browser state, and conversations in their operating-system application-data directories. A harness you run on another computer stores and processes information there, not necessarily on your own computer.

## Model requests and generated content

With a local model endpoint, inference requests go to the endpoint you configured, normally a loopback address on your device. If you configure a network address or another computer, that endpoint receives the requests.

With a remote model API, the provider receives the request content sent to it. This may include messages, instructions, selected attachments, tool results, file content, summaries, and feedback relevant to the current task, along with model settings and authentication information. The provider also receives ordinary connection information, such as your public IP address and request timing.

R3F Studio does not receive a separate copy of these requests through a built-in collection service. Providers control their own logging, retention, security, training practices, and geographic processing locations. Check their policies and account settings before sending confidential or personal information. Jynx cannot guarantee that a provider will not retain or use a request.

Message ratings or other local feedback may be stored by the client or included in later model context to guide responses. This is not a built-in upload of feedback to R3F Studio, and it does not by itself update model weights.

## Web searches, websites, and remote resources

When web tools are enabled, searches send query text to the search service used by the tool. Query text may be generated from your task or context. The built-in harness search currently uses DuckDuckGo. Opening or reading a page sends a request to that website. Text or files submitted through a browser action go to the destination you authorize.

Websites and services can receive your IP address, requested URLs, browser or HTTP-client details, and other normal connection data. A graphical browser can also use cookies, local storage, existing sign-ins, and website scripts according to its browser session and the site's behavior. Loading remote images, favicons, model cards, or avatars can contact their hosts even when you do not send a chat message.

Website operators and search services handle their own records and privacy choices. Content from those services is not controlled by R3F Studio.

## Optional voice input

If a graphical client offers voice input and you start it, it accesses the microphone and passes audio to the speech-recognition mechanism supported by your system or browser. That mechanism may use an external recognition service depending on the platform and its settings. Audio processing is not guaranteed to stay offline. The resulting transcript becomes composer or conversation content and is handled like other text you send to a model. Use your operating system's microphone and speech-recognition permissions to control access; do not start voice input if you do not want that processing.

## Updates, installation, and downloads

Downloading Jynx or an SDK contacts the host providing it, such as GitHub, GitHub Packages, npm, or PyPI. Those hosts handle download and account information under their own policies. Their handling is separate from Jynx's local conversation storage.

The CLI normally checks GitHub Releases for an update when launched interactively. The check sends a normal request to GitHub, including the CLI version in its user-agent. A subsequent update download uses the release asset appropriate to the version, operating system, and CPU architecture. These requests do not intentionally include your conversation or project files. Set `JYNX_NO_UPDATE_CHECK=1` to disable the CLI startup update check.

Clients and development or packaging tools may check or download application releases, llama.cpp runtimes, models, dependencies, and related resources from GitHub, Hugging Face, package registries, or other configured release hosts. Those hosts receive ordinary download request information. Installing software or viewing this notice does not authorize analytics collection or consent to every optional external feature.

## Tools, integrations, and application developers

MCP servers, plugins, extensions, commands, and other integrations may run locally or contact outside services. Their operators determine what they collect and retain. Permission to execute a tool can allow it to read, change, or transmit information within the limits of that tool and your operating-system permissions. A workspace selection is not a complete operating-system sandbox.

The JavaScript and Python SDKs send requests to the endpoints an integrating developer configures. The JavaScript harness client can start and control a local harness process; its tools and model configuration determine subsequent access and network activity. The SDKs do not add a built-in analytics or conversation-upload service for R3F Studio.

If another developer incorporates a Jynx SDK or harness into an application, that developer is responsible for the application's own data handling, notices, permissions, and provider choices. This policy does not describe extra collection they add, or services they operate.

## Voluntary contact and public project channels

R3F Studio can receive information you voluntarily submit through GitHub Issues, pull requests, discussions, or other project communications. Examples include your public account name, message content, code contributions, and files or diagnostics you attach. This is different from automatic collection by the installed software.

R3F Studio uses voluntarily submitted information to answer questions, investigate reported problems, maintain the project, and handle requests. GitHub controls the hosting of project communications and its account records. Public posts may be visible to other people, copied, indexed, or retained in platform history. Do not include passwords, API keys, private conversations, personal documents, or confidential source code in public reports.

## Retention and removal

Local conversations, settings, caches, downloaded models, and diagnostics remain on the machine or storage you configure until removed by you, the application, the operating system, or your storage-management tools. Backups or synchronized folders can retain additional copies under your control or the backup provider's control.

Uninstalling Jynx is intended to remove installed program files. It does not promise to erase all models, project files, conversation stores, browser data, credentials, backups, or information already sent to an external service. Use the relevant client's deletion controls and remove unwanted local state and backups separately. Review paths before deleting anything important.

R3F Studio cannot remotely read, export, or erase local data that it does not hold. Requests concerning a model provider, website, registry, or integration should also be directed to that operator. Voluntary project communications remain subject to the project's maintenance needs and the hosting platform's retention, deletion, and legal requirements; R3F Studio does not promise a universal retention period for records controlled by those platforms.

## Your choices

- Choose a genuinely local model endpoint when you want local inference, and check its address.
- Disable web search and browser tools when they are not needed. CLI web controls include `--no-web` and `--no-fetch`; see the usage documentation for their different effects.
- Use approval mode, inspect proposed actions, and restrict or remove integrations you do not trust. Approval is an execution control, not a guarantee that resulting content contains no personal information.
- Disable the CLI startup update check with `JYNX_NO_UPDATE_CHECK=1` when desired. This does not disable unrelated model, tool, registry, or operating-system requests.
- Review provider settings and permissions before using API keys, remote models, account-connected websites, or third-party services.
- Avoid putting secrets or personal information in prompts, attachments, diagnostics, or public issue reports unless needed and appropriate for the destination.

## Security

Jynx uses controls such as tool approvals and local credential storage where supported by the relevant client. In the official graphical client, configured API-model credentials use Electron's operating-system-backed secure-storage mechanism when available. Other credentials can be supplied through environment variables, configuration files, or integrations with different protections.

This does not mean all conversations, files, logs, or settings are encrypted. Anyone or any program with sufficient access to the host, backups, or configured services may be able to access them. No software, storage system, or network connection can be guaranteed secure.

## Personal information, legal rights, and international services

R3F Studio does not sell local Jynx conversations or use a built-in advertising-data collection system. If R3F Studio holds information you voluntarily provided, applicable law may give you rights concerning access, correction, deletion, objection, or other handling of that information. Which rights apply depends on your circumstances and the relevant law. R3F Studio will consider requests concerning information it actually holds; it cannot supply data that remained solely on your device.

External providers may process information in other countries and under different laws. Choosing a provider or integration can create an international transfer of the information sent to it. Review that provider's terms, settings, and privacy information for details.

Jynx is a general-purpose development tool, not a service specifically directed to children. R3F Studio does not intentionally solicit children's personal information through the software. Contact R3F Studio if you believe a child has submitted personal information to a project channel.

## Contact

For general privacy questions, use [R3F Studio's Jynx issue tracker](https://github.com/R3FStudio/Jynx/issues) and label the subject as a privacy question. Do not post personal information, credentials, or private records. If a request needs sensitive details, first ask for a suitable private contact method without including those details in the public issue. R3F Studio may need proportionate information to understand a request or establish that you are entitled to make it.

## Changes to this policy

The current policy is published at [Jynx's privacy policy](https://github.com/R3FStudio/Jynx/blob/main/PRIVACY.md). Changes will be recorded there with a revised effective date. Distribution packages contain the policy supplied with that build and may differ from the current online copy. If a future version introduces materially different data handling, its notice should explain that handling before the relevant feature is used. This policy does not replace the separate software license or remove rights granted by applicable law.
