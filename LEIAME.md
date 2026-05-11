# LEIAME - Maestro Editorial AI

Este pacote e portatil para Windows 11+. Ele nao instala servicos, nao cria entrada de instalador e nao deve gravar segredos no repositorio.

## Primeira execucao

1. Extraia o ZIP em uma pasta local.
2. Execute `Maestro Editorial AI.exe`.
3. Abra `Ajustes` e escolha o modo de persistencia.
4. Importe o protocolo editorial Markdown integral antes de iniciar uma sessao.
5. Use `Retomar` quando quiser continuar uma sessao interrompida salva em `data/sessions/`.
6. Abra `Setup` para conferir o estado de bootstrap e diagnostico.
7. Se algo falhar, envie o arquivo NDJSON mais recente de `data/logs/`.

## Arquivos locais criados pelo app

- `data/config/bootstrap.json`: arquivo local sem segredos. Ele informa ao app qual arranjo de configuracao foi escolhido.
- `data/config/ai-providers.json`: arquivo local das credenciais de API dos agentes quando o usuario clicar em `Salvar APIs`.
- `data/logs/maestro-<timestamp>-pid<id>.ndjson`: um arquivo novo por execucao do app.
- `data/sessions/<run>/`: prompt, protocolo fixado, saidas dos agentes, ata da sessao e texto final quando houver unanimidade.
- Cache e artefatos ficam sob `data/` e continuam fora do Git.

## Bootstrap de configuracao

O `bootstrap.json` sempre existe, mesmo quando o usuario escolhe Cloudflare. Ele guarda apenas ponteiros nao secretos:

- `credential_storage_mode`: `local_json`, `windows_env` ou `cloudflare`.
- `cloudflare_account_id`: identificador da conta, quando informado ou detectado.
- `cloudflare_api_token_source`: onde buscar o token inicial da Cloudflare.
- `cloudflare_api_token_env_var`: nome da env var que contem o token, quando aplicavel.
- `cloudflare_persistence_database`: `maestro_db`.
- `cloudflare_secret_store`: `maestro`.

O token da Cloudflare nao pode ficar apenas na Cloudflare, porque ele e necessario antes de o app conseguir entrar na Cloudflare. Por isso o acesso inicial deve usar uma destas opcoes:

- Env var do Windows, recomendado para testes atuais.
- Campo temporario no app a cada execucao.
- Cofre local criptografado por usuario do Windows, planejado para versao futura.

## Env vars Cloudflare lidas automaticamente

Em toda abertura o app procura estas variaveis:

- Account ID: `MAESTRO_CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_ACCOUNT_ID`, `CF_ACCOUNT_ID`.
- API token: `MAESTRO_CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_API_TOKEN`, `CF_API_TOKEN`.

O app pode preencher o Account ID na tela. O valor do token nao e exibido nem gravado em log; o app registra apenas se ele existe e o nome da env var usada.

Na validacao real, o Maestro diferencia tokens de usuario (`cfut_`) e tokens de conta (`cfat_`). Tokens de conta exigem Account ID e sao verificados pelo endpoint de conta da Cloudflare. A tela informa se a env var foi vista pelo processo, pelo escopo de usuario ou pelo escopo de maquina.

O botao `Verificar e preparar` valida o token e tenta preparar os recursos essenciais quando estiverem ausentes: D1 `maestro_db` e tabelas internas do Maestro. Para Secrets Store, o app primeiro usa qualquer store ja existente na conta, sem renomear. Ele so tenta criar `maestro` quando nenhum Secrets Store existir. Se o token nao tiver permissao para criar algum recurso, a tela mostra a falha no item correspondente.

## APIs dos agentes

Em `Ajustes > Agentes via API`, informe as chaves e clique em `Salvar APIs` para gravar o estado local. O app mostra uma mensagem de status informando quando salvou. `Verificar APIs` salva antes de testar e consulta endpoints reais de listagem de modelos da OpenAI, Anthropic, Gemini e DeepSeek. As chaves nao sao gravadas nos logs.

DeepSeek e Grok usam API, nao CLI local. Para que operem sem digitar a chave a cada execucao, configure `MAESTRO_DEEPSEEK_API_KEY` / `DEEPSEEK_API_KEY` e `MAESTRO_GROK_API_KEY` / `GROK_API_KEY` / `XAI_API_KEY` no Windows, ou salve as chaves pelo fluxo de APIs do app.

Quando qualquer peer rodar via API, defina tambem as tarifas de entrada/saida e o limite maximo de custo da sessao em USD antes de iniciar ou retomar. O Maestro nao executa chamada paga sem teto de custo informado pelo usuario.
Em retomadas, esse teto vale para a tentativa atual: custos de execucoes anteriores ficam preservados no historico, inclusive historicos legados sem identificador de execucao, mas nao consomem o novo limite informado.

Quando o provedor oferece cache de prompt sem degradar qualidade, o Maestro usa isso apenas para reduzir custo: OpenAI/Codex e Grok/xAI enviam `prompt_cache_key`, Anthropic/Claude marca o bloco `system` estavel com cache efemero, e Gemini/DeepSeek usam cache implicito/automatico do provedor. Logs e artefatos gravam somente hash/metadados nao secretos e contadores de tokens de cache quando o provedor retorna esses dados.

## Modos de persistencia

- JSON local: configuracoes e segredos ficam em arquivos locais sob `data/`, com aviso de cuidado operacional.
- Env var Windows: tokens e API keys ficam em env vars; demais configuracoes ficam no JSON local.
- Cloudflare: configuracoes remotas ficam em D1 `maestro_db`; segredos de agentes ficam no Cloudflare Secrets Store. Ainda e necessario um segredo de bootstrap local ou digitado para entrar na Cloudflare. A Cloudflare nao devolve o valor bruto de um segredo ja salvo no Secrets Store; ao reabrir, o Maestro recupera as referencias remotas e usa chaves locais/env vars quando precisar executar uma chamada diretamente do desktop.

## Estado deste build

Marco operacional: a versao `0.5.25` e a primeira versao documentada como baseline operacional estavel. A versao `0.5.26` preserva esse baseline e adiciona hardening de auditoria: escrita atomica de artefatos/configuracoes/anexos, classificacao operacional de erros HTTP dos provedores, logs de erro sem URLs de request, diagnostico de persistencia parcial no Cloudflare Secrets Store e gates de CI/release mais fortes. A sessao real `run-2026-05-11T01-09-30-556Z`, retomada em modo API com Claude, Codex, Gemini, DeepSeek e Grok, terminou em `READY_UNANIMOUS` e gerou `texto-final.md` limpo, sem marcadores tecnicos internos.

Este build executa sessao editorial real em background: Claude, Codex, Gemini, DeepSeek e Grok podem gerar/revisar o texto contra o protocolo integral importado quando o modo selecionado permitir esses peers. DeepSeek e Grok participam em API/Hibrido e ficam desativados em CLI. O agente que escreveu o rascunho ou a revisao atual nao revisa o proprio texto; por isso, selecione pelo menos dois agentes ativos para que exista revisao independente. Se nao houver revisor independente, a sessao pausa claramente e pode ser retomada depois de ajustar os agentes ativos. Se um agente nao retornar aprovacao, a sessao nao deve ser tratada como finalizada; ela permanece sem entrega final e novas rodadas de revisao devem continuar ate unanimidade. A ata fica disponivel em `data/sessions/<run>/ata-da-sessao.md` e agrupa os eventos por rodada real. O texto publico final, quando houver unanimidade, fica em `data/sessions/<run>/texto-final.md` sem o marcador tecnico interno `MAESTRO_STATUS`.

As chamadas editoriais reais nao possuem timeout artificial, porque os modelos podem demorar bastante para cumprir o protocolo. A UI mostra andamento e tempo decorrido enquanto os agentes trabalham, e as CLIs devem rodar sem qualquer janela de terminal visivel.

Para retomar uma sessao interrompida, clique em `Retomar`. O Maestro le `data/sessions/`; se houver uma sessao disponivel, continua automaticamente; se houver varias, pede para escolher. Se voce importar um novo protocolo antes de retomar, ele sera enviado aos agentes e o protocolo anterior sera preservado como artefato local da sessao. Se nao houver novo protocolo carregado, o app usa o `protocolo.md` salvo dentro da sessao.

Os logs foram ampliados para diagnostico: eles registram contexto de UI, estado do runtime, caminhos resolvidos das CLIs, inicio/fim de cada agente, duracao, exit code, politica de timeout e caminho dos artefatos. O conteudo bruto dos agentes fica nos arquivos de sessao, nao embutido no NDJSON geral.

Regra inviolavel: nenhum texto final deve ser entregue sem unanimidade real dos revisores independentes entre os agentes ativos.

Se todos os revisores independentes falharem por motivo operacional em rodadas repetidas, o Maestro pausa a sessao de forma recuperavel com orientacao para tentar novamente, trocar o modo de transporte ou habilitar mais revisores. Essa pausa nao reescreve o texto atual e nao autoriza auto-revisao do agente que redigiu ou retomou a versao em analise.
