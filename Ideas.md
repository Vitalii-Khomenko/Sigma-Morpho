### Техническое задание: NeuroBuster (SNN-Driven Web Fuzzer)

**1. Цель проекта**
Исследование применимости спайковых нейронных сетей (SNN) для адаптивного перебора директорий (directory busting) и обхода средств защиты (WAF/IPS). SNN используется не для генерации текста, а для анализа временных паттернов ответов сервера и динамической корректировки задержек и приоритетов путей.

**2. Архитектура системы**
* **Сетевой модуль (Rust/Tokio + Reqwest):** Асинхронный движок отправки HTTP-запросов.
* **Энкодер данных:** Преобразование параметров ответа сервера (HTTP статус, размер тела, время отклика) во входные спайки (spike trains).
* **Ядро SNN (LIF модель):** Нейроморфная сеть, состоящая из Leaky Integrate-and-Fire (LIF) нейронов. Обрабатывает спайки во времени.
* **Декодер (Политика действий):** Анализ выходных спайков сети для изменения стратегии:
    * Увеличение/уменьшение паузы между запросами (WAF evasion).
    * Смена словаря или мутация текущего пути.

**3. Принцип взаимодействия (Спайковая логика)**
* *Вход:* Если сервер отвечает 429 (Too Many Requests) или резко возрастает задержка (Latency), энкодер генерирует высокочастотный спайк на тормозящие (inhibitory) нейроны SNN.
* *Обработка:* Потенциал мембраны накапливается/затухает.
* *Выход:* Если срабатывают определенные выходные нейроны, система модифицирует поведение (например, переходит в "stealth" режим).

---

### Начальный код (Rust)

**1. Зависимости (`Cargo.toml`)**
```toml
[package]
name = "neurobuster"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
rand = "0.8"
```

**2. Реализация (`src/main.rs`)**
```rust
use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::time::sleep;

// Параметры LIF-нейрона
const THRESHOLD: f32 = 1.0;
const LEAK_RATE: f32 = 0.9; // Скорость затухания потенциала
const REFRACTORY_PERIOD: u8 = 2; // Тики отдыха после спайка

struct LifNeuron {
    potential: f32,
    refractory_timer: u8,
}

impl LifNeuron {
    fn new() -> Self {
        Self { potential: 0.0, refractory_timer: 0 }
    }

    // Возвращает true, если произошел спайк
    fn step(&mut self, input_current: f32) -> bool {
        if self.refractory_timer > 0 {
            self.refractory_timer -= 1;
            return false;
        }

        self.potential = (self.potential * LEAK_RATE) + input_current;

        if self.potential >= THRESHOLD {
            self.potential = 0.0; // Сброс после спайка
            self.refractory_timer = REFRACTORY_PERIOD;
            return true;
        }
        false
    }
}

// Упрощенная SNN для контроля рейт-лимитов
struct ThrottleSNN {
    stress_neuron: LifNeuron,
}

impl ThrottleSNN {
    fn new() -> Self {
        Self {
            stress_neuron: LifNeuron::new(),
        }
    }

    // Оценка угрозы WAF на основе ответа
    fn process_response(&mut self, status: u16, latency_ms: u64) -> u64 {
        let mut input_current = 0.0;

        // Энкодер: конвертация метрик в ток
        if status == 429 || status == 403 {
            input_current += 0.8; // Высокий стресс от блокировок
        }
        if latency_ms > 1000 {
            input_current += 0.3; // Аномальная задержка (возможно, tarpit)
        }

        // Если нейрон спайкует, мы детектируем защиту и увеличиваем задержку
        let is_spiking = self.stress_neuron.step(input_current);
        
        if is_spiking {
            println!("[SNN SPIKE] WAF activity detected. Adapting throttle.");
            5000 // Задержка 5 секунд (адаптация)
        } else {
            100  // Стандартная задержка 100 мс
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    
    let target_url = "http://localhost:8000"; // Заменить на таргет HTB
    let wordlist = vec!["admin", "login", "api", "hidden_test_dir"]; 
    
    let mut snn = ThrottleSNN::new();
    let mut current_delay = 100;

    for word in wordlist {
        let url = format!("{}/{}", target_url, word);
        let start = Instant::now();
        
        match client.get(&url).send().await {
            Ok(response) => {
                let latency = start.elapsed().as_millis() as u64;
                let status = response.status().as_u16();
                
                println!("[{}] {} - Latency: {}ms", status, url, latency);
                
                // Пропускаем данные через SNN для вычисления новой задержки
                current_delay = snn.process_response(status, latency);
            }
            Err(e) => {
                println!("[ERR] {} - {:?}", url, e);
                // Сетевые ошибки также вызывают стресс сети
                current_delay = snn.process_response(0, 5000); 
            }
        }

        sleep(Duration::from_millis(current_delay)).await;
    }

    Ok(())
}
```

Этот код представляет базовый фундамент. Для полноценных экспериментов потребуется расширить архитектуру SNN до многослойной сети, где веса (синапсы) обновляются с помощью механизмов STDP (Spike-Timing-Dependent Plasticity) для запоминания паттернов WAF на лету.

### Архитектура NeuroBuster 2.0: Многослойная SNN и STDP

Для перехода от жестко заданных правил к самообучению внедряем синаптические веса и механизм **STDP (Spike-Timing-Dependent Plasticity)**. Сеть будет "запоминать" паттерны, приводящие к блокировке (например, серия из трех 404 ошибок, за которой следует резкий рост пинга), и превентивно снижать скорость.

**Математическая модель STDP:**
Вес синапса $w$ обновляется в зависимости от разницы во времени $\Delta t = t_{post} - t_{pre}$ между спайками пре- и постсинаптического нейронов:
$$\Delta w = \begin{cases} A_+ e^{-\Delta t / \tau_+}, & \text{если } \Delta t > 0 \text{ (усиление связи)} \\ -A_- e^{\Delta t / \tau_-}, & \text{если } \Delta t < 0 \text{ (ослабление связи)} \end{cases}$$

---

### Расширенная реализация на Rust

Добавлен синаптический слой, история спайков и функция обучения (LTP/LTD).

**`src/snn.rs` (Ядро сети)**

```rust
use std::collections::VecDeque;

const THRESHOLD: f32 = 1.0;
const LEAK_RATE: f32 = 0.85;
const LEARNING_RATE: f32 = 0.1;
const MAX_WEIGHT: f32 = 2.0;

#[derive(Clone)]
pub struct Synapse {
    pub weight: f32,
    pub pre_neuron_idx: usize,
}

pub struct LifNeuron {
    potential: f32,
    pub spike_history: VecDeque<u64>, // Хранение временных меток спайков для STDP
}

impl LifNeuron {
    pub fn new() -> Self {
        Self {
            potential: 0.0,
            spike_history: VecDeque::with_capacity(10),
        }
    }

    pub fn step(&mut self, current: f32, tick: u64) -> bool {
        self.potential = self.potential * LEAK_RATE + current;
        if self.potential >= THRESHOLD {
            self.potential = 0.0;
            self.spike_history.push_back(tick);
            if self.spike_history.len() > 10 {
                self.spike_history.pop_front();
            }
            return true;
        }
        false
    }
}

pub struct WafEvasionNetwork {
    input_neurons: Vec<LifNeuron>,  // 0: Latency, 1: 429/403, 2: 404 Rate
    output_neuron: LifNeuron,       // Нейрон "Паники" (увеличить задержку)
    synapses: Vec<Synapse>,
    global_tick: u64,
}

impl WafEvasionNetwork {
    pub fn new() -> Self {
        Self {
            input_neurons: vec![LifNeuron::new(), LifNeuron::new(), LifNeuron::new()],
            output_neuron: LifNeuron::new(),
            synapses: vec![
                Synapse { weight: 0.5, pre_neuron_idx: 0 },
                Synapse { weight: 0.8, pre_neuron_idx: 1 },
                Synapse { weight: 0.2, pre_neuron_idx: 2 },
            ],
            global_tick: 0,
        }
    }

    pub fn process_metrics(&mut self, latency_ms: u64, status: u16, tick_delta: u64) -> u64 {
        self.global_tick += tick_delta;
        
        // 1. Энкодинг метрик во входные токи
        let i_latency = if latency_ms > 800 { 0.6 } else { 0.0 };
        let i_block = if status == 429 || status == 403 { 1.2 } else { 0.0 };
        let i_not_found = if status == 404 { 0.3 } else { 0.0 };

        let inputs = [i_latency, i_block, i_not_found];
        let mut out_current = 0.0;

        // 2. Шаг входных нейронов
        for (i, neuron) in self.input_neurons.iter_mut().enumerate() {
            if neuron.step(inputs[i], self.global_tick) {
                out_current += self.synapses[i].weight;
            }
        }

        // 3. Шаг выходного нейрона (Decision)
        let is_evasion_needed = self.output_neuron.step(out_current, self.global_tick);

        // 4. STDP Обучение (если произошел блок - наказываем/поощряем синапсы)
        if status == 429 {
            self.apply_stdp();
        }

        if is_evasion_needed {
            println!("[!] SNN Threshold reached. Applying evasive maneuvers.");
            3000 // Динамическая задержка
        } else {
            50   // Агрессивный перебор
        }
    }

    // Упрощенная реализация STDP
    fn apply_stdp(&mut self) {
        let post_spikes = &self.output_neuron.spike_history;
        if post_spikes.is_empty() { return; }
        let last_post = *post_spikes.back().unwrap();

        for (i, syn) in self.synapses.iter_mut().enumerate() {
            let pre_spikes = &self.input_neurons[i].spike_history;
            if let Some(&last_pre) = pre_spikes.back() {
                let delta_t = last_post as i64 - last_pre as i64;
                
                // Если пре-синаптический спайк был прямо перед блоком -> усиливаем связь
                if delta_t > 0 && delta_t < 50 {
                    syn.weight = (syn.weight + LEARNING_RATE).min(MAX_WEIGHT);
                } 
                // Если пре-синаптический спайк был после (или не связан) -> ослабляем
                else {
                    syn.weight = (syn.weight - (LEARNING_RATE / 2.0)).max(0.1);
                }
            }
        }
    }
}
```

### Интеграция в цикл (Сбор метрик)

В основном модуле `main.rs` логика становится потоковой:

```rust
// ... инициализация
let mut snn = WafEvasionNetwork::new();
let mut delay = 50;

for word in wordlist {
    let url = format!("{}/{}", target_url, word);
    let start = Instant::now();
    
    // Выполнение запроса
    let resp = client.get(&url).send().await;
    let latency = start.elapsed().as_millis() as u64;
    let status = resp.map(|r| r.status().as_u16()).unwrap_or(000);
    
    // Сеть обрабатывает данные и самообучается, выдавая новую задержку
    delay = snn.process_metrics(latency, status, 1);
    
    sleep(Duration::from_millis(delay)).await;
}
```

### Векторы дальнейшего усложнения
1. **Топология:** Переход от feed-forward к Recurrent SNN (RSNN) для обработки длинных временных последовательностей ответов.
2. **Rotator IP/Agent:** Добавление выходных нейронов для триггера смены User-Agent или переключения прокси-узла (Tor circuit rebuild), если нейрон задержки перегружен.
3. **Rust-оптимизация:** Перенос вычислений потенциала мембраны на SIMD инструкции (crate `std::simd`), если размер словаря превышает миллионы строк и сеть содержит сотни нейронов.


### Архитектура Recurrent SNN (RSNN) для анализа последовательностей

Переход к RSNN позволяет сети удерживать контекст предыдущих запросов (краткосрочную память). Это критично для выявления сложных правил WAF, таких как "блокировка после 5 ошибок 404 за 10 секунд". В архитектуру добавляется скрытый слой нейронов, соединенных как с входами, так и между собой (рекуррентные синапсы).

**Математическая модель скрытого слоя:**
Потенциал мембраны нейрона $i$ в момент времени $t$ вычисляется с учетом входного сигнала и спайков соседних нейронов на предыдущем шаге $t-1$:
$$V_i(t) = V_i(t-1) \cdot \lambda + \sum_{j} W_{ji}^{in} X_j(t) + \sum_{k} W_{ki}^{rec} S_k(t-1)$$
Где:
* $\lambda$ — коэффициент утечки (leak rate).
* $W^{in}$, $W^{rec}$ — веса входных и рекуррентных синапсов.
* $X(t)$ — входные токи.
* $S(t-1) \in \{0, 1\}$ — наличие спайка рекуррентного нейрона на прошлом шаге.

---

### Реализация RSNN (Rust)

Код оптимизирован для высокой производительности. Векторы используются для матричных операций передачи сигналов.

**`src/rsnn.rs`**

```rust
use rand::Rng;

const THRESHOLD: f32 = 1.0;
const LEAK_RATE: f32 = 0.85;

#[derive(Clone)]
pub struct Neuron {
    pub potential: f32,
}

impl Neuron {
    fn new() -> Self { Self { potential: 0.0 } }
}

pub struct RSNN {
    hidden_neurons: Vec<Neuron>,
    input_weights: Vec<Vec<f32>>,      // [input_idx][hidden_idx]
    recurrent_weights: Vec<Vec<f32>>,  // [hidden_idx_from][hidden_idx_to]
    output_weights: Vec<f32>,          // [hidden_idx]
    last_spikes: Vec<bool>,            // S(t-1)
    output_neuron: Neuron,
}

impl RSNN {
    pub fn new(input_size: usize, hidden_size: usize) -> Self {
        let mut rng = rand::thread_rng();

        // Инициализация случайных весов для создания "резервуара" (Liquid State Machine)
        let input_weights = (0..input_size)
            .map(|_| (0..hidden_size).map(|_| rng.gen_range(0.1..0.8)).collect())
            .collect();

        let recurrent_weights = (0..hidden_size)
            .map(|i| {
                (0..hidden_size)
                    .map(|j| if i == j { 0.0 } else { rng.gen_range(-0.5..0.5) }) // Ингибиторные и эксцитаторные связи
                    .collect()
            })
            .collect();

        let output_weights = (0..hidden_size).map(|_| rng.gen_range(0.2..1.0)).collect();

        Self {
            hidden_neurons: vec![Neuron::new(); hidden_size],
            input_weights,
            recurrent_weights,
            output_weights,
            last_spikes: vec![false; hidden_size],
            output_neuron: Neuron::new(),
        }
    }

    // Возвращает true, если требуется смена тактики (эвазия)
    pub fn process_step(&mut self, inputs: &[f32]) -> bool {
        let hidden_size = self.hidden_neurons.len();
        let mut current_spikes = vec![false; hidden_size];

        // 1. Интеграция токов в скрытом слое
        for i in 0..hidden_size {
            let mut current = 0.0;

            // Ток от метрик (Входы)
            for j in 0..inputs.len() {
                current += inputs[j] * self.input_weights[j][i];
            }

            // Ток от рекуррентных связей (Память сети)
            for k in 0..hidden_size {
                if self.last_spikes[k] {
                    current += self.recurrent_weights[k][i];
                }
            }

            // Обновление мембраны нейрона i
            let n = &mut self.hidden_neurons[i];
            n.potential = (n.potential * LEAK_RATE) + current;

            // Генерация спайка
            if n.potential >= THRESHOLD {
                n.potential = 0.0; // Сброс
                current_spikes[i] = true;
            }
        }

        self.last_spikes = current_spikes.clone();

        // 2. Агрегация на выходном нейроне (Decision Layer)
        let mut out_current = 0.0;
        for i in 0..hidden_size {
            if current_spikes[i] {
                out_current += self.output_weights[i];
            }
        }

        self.output_neuron.potential = (self.output_neuron.potential * LEAK_RATE) + out_current;

        if self.output_neuron.potential >= THRESHOLD {
            self.output_neuron.potential = 0.0;
            return true;
        }

        false
    }
}
```

### Интеграция в цикл сканирования

В основном процессе `main.rs` требуется только передавать массив нормированных метрик на каждом запросе.

```rust
// Инициализация: 3 входа (Latency, 429/403, 404), 10 рекуррентных нейронов
let mut rsnn = RSNN::new(3, 10);
let mut delay = 50;

// ... внутри цикла перебора:

let i_latency = if latency > 800 { 0.8 } else { 0.0 };
let i_block = if status == 429 || status == 403 { 1.5 } else { 0.0 };
let i_not_found = if status == 404 { 0.4 } else { 0.0 };

let inputs = vec![i_latency, i_block, i_not_found];

let evasion_triggered = rsnn.process_step(&inputs);

if evasion_triggered {
    println!("[RSNN] Sequence anomaly detected. Adjusting rate.");
    delay = 5000; // Резкое снижение скорости
} else {
    // Постепенное восстановление скорости, если сеть "успокаивается"
    delay = std::cmp::max(50, delay - 100); 
}
```

Данная архитектура способна аккумулировать возбуждение от серии безобидных на первый взгляд ответов (например, частых 404), которые в комбинации с небольшим ростом задержки могут свидетельствовать о работе эвристического анализатора IPS, что позволяет превентивно замедлить скан до получения аппаратной блокировки.

### Архитектура модуля Rotator (IP/User-Agent)

Внедряем дополнительные выходные нейроны. У каждого свой порог срабатывания (Threshold) и уровень утечки (Leak Rate), что создает иерархию эвазии:
1. **Уровень 1 (Задержка):** Низкий порог. Быстро реагирует на микро-аномалии.
2. **Уровень 2 (Смена User-Agent):** Средний порог. Срабатывает при устойчивом паттерне 403 Forbidden.
3. **Уровень 3 (Смена IP / Tor NewNym):** Высокий порог. Триггерится только при перегрузке нейронов 1 и 2 (аппаратный бан, 429 Too Many Requests).

---

### Реализация на Rust

**1. Определение действий и расширение ядра SNN (`src/rsnn.rs`)**

```rust
#[derive(Debug, PartialEq)]
pub enum EvasionAction {
    Throttle(u64),
    RotateUserAgent,
    RebuildTorCircuit,
}

pub struct EvasionOutputs {
    throttle_neuron: Neuron,
    ua_neuron: Neuron,
    ip_neuron: Neuron,
}

impl RSNN {
    // ... инициализация скрытых слоев (как в предыдущем шаге) ...

    pub fn process_step(&mut self, inputs: &[f32]) -> Vec<EvasionAction> {
        // ... (шаг скрытого слоя) ...

        let mut actions = Vec::new();

        // Токи для выходных нейронов (упрощенная агрегация для примера)
        let out_current_throttle = self.calculate_output_current(0); // Связь с метриками задержки
        let out_current_ua = self.calculate_output_current(1);       // Связь с 403
        let out_current_ip = self.calculate_output_current(2);       // Связь с 429 и перегрузкой

        // 1. Проверка Throttle (Порог 1.0, Утечка 0.8)
        self.outputs.throttle_neuron.potential = self.outputs.throttle_neuron.potential * 0.8 + out_current_throttle;
        if self.outputs.throttle_neuron.potential >= 1.0 {
            self.outputs.throttle_neuron.potential = 0.0;
            actions.push(EvasionAction::Throttle(3000)); 
        }

        // 2. Проверка User-Agent (Порог 2.0, Утечка 0.9 - требует плотного потока спайков)
        self.outputs.ua_neuron.potential = self.outputs.ua_neuron.potential * 0.9 + out_current_ua;
        if self.outputs.ua_neuron.potential >= 2.0 {
            self.outputs.ua_neuron.potential = 0.0;
            actions.push(EvasionAction::RotateUserAgent);
        }

        // 3. Проверка IP / Tor (Порог 3.5, Утечка 0.95 - хард-лимит)
        self.outputs.ip_neuron.potential = self.outputs.ip_neuron.potential * 0.95 + out_current_ip;
        if self.outputs.ip_neuron.potential >= 3.5 {
            self.outputs.ip_neuron.potential = 0.0;
            self.outputs.ua_neuron.potential = 0.0; // Сброс каскада
            self.outputs.throttle_neuron.potential = 0.0;
            actions.push(EvasionAction::RebuildTorCircuit);
        }

        actions
    }
}
```

**2. Менеджер ротации (Сетевой контроллер) (`src/rotator.rs`)**

Для смены IP используется отправка сигнала `NEWNYM` в Control Port Tor (обычно 9051).

```rust
use reqwest::{Client, Proxy};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use rand::seq::SliceRandom;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36...",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15...",
    "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/111.0",
];

pub struct Rotator {
    current_ua_idx: usize,
    tor_proxy_url: String,
}

impl Rotator {
    pub fn new(tor_proxy_url: &str) -> Self {
        Self {
            current_ua_idx: 0,
            tor_proxy_url: tor_proxy_url.to_string(),
        }
    }

    // Сборка нового HTTP-клиента с текущими настройками
    pub fn build_client(&self) -> Client {
        let proxy = Proxy::all(&self.tor_proxy_url).unwrap();
        Client::builder()
            .proxy(proxy)
            .user_agent(USER_AGENTS[self.current_ua_idx])
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap()
    }

    pub fn rotate_ua(&mut self) {
        let mut rng = rand::thread_rng();
        self.current_ua_idx = *vec![0, 1, 2].choose(&mut rng).unwrap();
        println!("[ROTATOR] User-Agent switched to index: {}", self.current_ua_idx);
    }

    // Отправка команды на смена цепочки узлов (Tor Circuit)
    pub async fn rebuild_tor_circuit(&self, control_port: &str, password: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("[ROTATOR] CRITICAL: Rebuilding Tor Circuit (NEWNYM)...");
        let mut stream = TcpStream::connect(control_port).await?;
        
        let auth_cmd = format!("AUTHENTICATE \"{}\"\r\n", password);
        stream.write_all(auth_cmd.as_bytes()).await?;
        
        let signal_cmd = "SIGNAL NEWNYM\r\n";
        stream.write_all(signal_cmd.as_bytes()).await?;
        
        // Пауза для перестройки маршрута
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        Ok(())
    }
}
```

**3. Интеграция в Event Loop (`src/main.rs`)**

```rust
let mut rsnn = RSNN::new(3, 10);
let mut rotator = Rotator::new("socks5h://127.0.0.1:9050");
let mut client = rotator.build_client();
let mut delay = 50;

for word in wordlist {
    // ... (отправка запроса через текущий клиент) ...
    
    // Получение команд от сети
    let actions = rsnn.process_step(&inputs);

    for action in actions {
        match action {
            EvasionAction::Throttle(ms) => {
                println!("[ACTION] Throttling for {}ms", ms);
                delay = ms;
            }
            EvasionAction::RotateUserAgent => {
                rotator.rotate_ua();
                client = rotator.build_client(); // Пересборка клиента
            }
            EvasionAction::RebuildTorCircuit => {
                // Триггер Tor Control Port
                if let Err(e) = rotator.rebuild_tor_circuit("127.0.0.1:9051", "tor_password").await {
                    eprintln!("[ERR] Failed to rebuild Tor: {}", e);
                }
                client = rotator.build_client();
                delay = 5000; // Ожидание после смены IP
            }
        }
    }
    
    sleep(Duration::from_millis(delay)).await;
    delay = std::cmp::max(50, delay.saturating_sub(100)); // Затухание задержки
}
```


Да, этот проект идеально ложится на экосистему Rust. Асинхронность `tokio` в связке с вычислительной производительностью Rust отлично подходит для высоконагруженного сетевого ввода-вывода (I/O) и симуляции нейросетей (CPU-bound) в реальном времени.

Ниже представлена полная структура проекта и техническое задание на английском языке.

### Структура проекта (Project Tree)

```text
neurobuster/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Точка входа, инициализация CLI и логгера
│   ├── cli.rs                  # Парсинг аргументов командной строки (clap)
│   ├── core/
│   │   ├── mod.rs
│   │   ├── engine.rs           # Главный цикл фаззинга, координация потоков
│   │   └── metrics.rs          # Сбор телеметрии (latency, status codes)
│   ├── network/
│   │   ├── mod.rs
│   │   ├── client.rs           # Обертка над reqwest, управление пулом соединений
│   │   └── rotator.rs          # Логика смены User-Agent и перестройки Tor circuit
│   └── neuro/
│       ├── mod.rs
│       ├── encoder.rs          # Преобразование HTTP-метрик во входные токи
│       ├── neuron.rs           # Модель LIF нейрона (Leaky Integrate-and-Fire)
│       ├── synapse.rs          # Синапсы и механизм STDP (Spike-Timing-Dependent Plasticity)
│       └── rsnn.rs             # Recurrent Spiking Neural Network (Топология и инференс)
```

---

### Technical Specification: NeuroBuster (Pure Rust SNN-Driven Web Fuzzer)

#### 1. System Overview
NeuroBuster is an advanced, asynchronous web directory brute-forcing and fuzzing tool written entirely in Rust. It integrates a Recurrent Spiking Neural Network (RSNN) to dynamically analyze server responses (latency, HTTP status codes) and autonomously adapt its operational parameters to evade Web Application Firewalls (WAF) and Intrusion Prevention Systems (IPS).

#### 2. Core Architecture Modules

* **`cli` (Command Line Interface):** * Utilizes the `clap` crate for parsing targets, wordlists, threading limits, and Tor control port configurations.
* **`core::engine` (Execution Engine):** * Manages the asynchronous execution environment (`tokio`).
    * Implements a producer-consumer pattern. The producer feeds paths from the wordlist; consumers execute HTTP requests governed by the SNN's dynamic delay state.
* **`neuro::rsnn` (Neuromorphic Core):** * A custom implementation of a Recurrent Spiking Neural Network.
    * Maintains temporal context of server behavior to detect sequence anomalies (e.g., tarpits, rate-limit thresholds).
    * Calculates the required delay (`throttle`) and triggers categorical evasion maneuvers.
* **`network::rotator` (Evasion Controller):** * Executes actions dictated by the SNN.
    * Manages HTTP client state. Capable of on-the-fly User-Agent rotation and issuing `SIGNAL NEWNYM` to the Tor Control Port via raw TCP sockets to acquire a new IP address.

#### 3. Neuromorphic Engine Specifications

* **Neuron Model:** Leaky Integrate-and-Fire (LIF) with defined resting potentials, threshold values, and refractory periods.
* **Topology:**
    * **Input Layer:** 3 Nodes (Latency Anomaly, 429/403 Rate, 404 Rate).
    * **Hidden Layer:** Fully connected recurrent pool (Liquid State Machine approach) to retain short-term temporal memory of the request-response sequence.
    * **Output Layer:** 3 Action Nodes (Throttle Adjust, Rotate UA, Rebuild Circuit).
* **Plasticity (STDP):** Synaptic weights adjust dynamically. If an action results in successful evasion (return to 200/404 baseline), the weights responsible for that action are reinforced.

#### 4. Concurrency & State Management

* **Runtime:** `tokio` multi-threaded runtime.
* **State Sharing:** The RSNN state is heavily mutable. It must be wrapped in `Arc<Mutex<RSNN>>` or `Arc<RwLock<RSNN>>` to allow thread-safe metric feeding and action retrieval across thousands of asynchronous HTTP worker tasks. 
* **Channel Communication:** Use `tokio::sync::mpsc` for the metric pipeline. Workers send execution results (latency, status) to a dedicated "Neuro-Thread" which processes the RSNN steps and broadcasts the updated target delay via `tokio::sync::watch`.

#### 5. Recommended Crates (`Cargo.toml` dependencies)

```toml
[dependencies]
# Async Runtime & Networking
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["rustls-tls", "socks"] }

# CLI & Configuration
clap = { version = "4", features = ["derive"] }

# Utilities & Math
rand = "0.8"
parking_lot = "0.12" # Faster Mutex/RwLock for SNN state
```


Главное, чего не хватает для превращения прототипа в реальный боевой инструмент — **асинхронного движка с высокой конкурентностью (Concurrency) и правильного распределения состояний**. 

В текущем виде запросы идут последовательно. В реальности fuzzer должен генерировать тысячи запросов в секунду. Однако мы не можем просто обернуть `RSNN` в `Mutex`, так как это создаст жесточайшее бутылочное горлышко (lock contention). Нейронная сеть должна работать как **изолированный Actor**, общающийся с воркерами через каналы.

Кроме того, вычисления SNN — это CPU-bound задача (перемножение матриц), поэтому ее нужно выносить на отдельный тред, чтобы не блокировать асинхронный I/O рантайм `tokio`.

Вот реализация недостающего ядра: **Neuro-Actor Engine**.

### 1. Архитектура каналов (Message Passing)
* **MPSC (Multi-Producer, Single-Consumer):** Воркеры отправляют метрики (статус, пинг) в нейро-ядро.
* **Watch Channel (Single-Producer, Multi-Consumer):** Нейро-ядро броадкастит актуальную задержку (throttle) всем воркерам одновременно.

### 2. Реализация `src/core/engine.rs`

```rust
use tokio::sync::{mpsc, watch};
use tokio::task;
use std::time::Duration;

// Структура метрики от HTTP-воркера
pub struct ResponseMetric {
    pub latency_ms: u64,
    pub status: u16,
}

// Запуск изолированного нейро-ядра (Actor)
pub fn spawn_neuro_actor(
    mut metric_rx: mpsc::Receiver<ResponseMetric>,
    delay_tx: watch::Sender<u64>,
) {
    // Используем spawn_blocking, т.к. расчеты нейросети - это тяжелая математика (CPU-bound),
    // которая не должна тормозить асинхронный цикл ввода-вывода (I/O).
    task::spawn_blocking(move || {
        let mut rsnn = crate::neuro::rsnn::RSNN::new(3, 20); // 3 входа, 20 скрытых
        let mut current_delay = 50; // Базовая задержка

        // Блокирующее чтение из канала метрик
        while let Some(metric) = metric_rx.blocking_recv() {
            // Конвертация метрик в токи
            let i_latency = if metric.latency_ms > 800 { 0.8 } else { 0.0 };
            let i_block = if metric.status == 429 || metric.status == 403 { 1.5 } else { 0.0 };
            let i_not_found = if metric.status == 404 { 0.4 } else { 0.0 };

            let actions = rsnn.process_step(&[i_latency, i_block, i_not_found]);

            let mut delay_changed = false;

            for action in actions {
                match action {
                    crate::neuro::rsnn::EvasionAction::Throttle(ms) => {
                        current_delay = ms;
                        delay_changed = true;
                    }
                    crate::neuro::rsnn::EvasionAction::RotateUserAgent => {
                        println!("[NEURO] Signal: Rotate UA");
                        // Здесь можно послать сигнал в отдельный канал ротатора
                    }
                    crate::neuro::rsnn::EvasionAction::RebuildTorCircuit => {
                        println!("[NEURO] Signal: Rebuild Tor Circuit");
                        current_delay = 5000;
                        delay_changed = true;
                    }
                }
            }

            // Постепенное "остывание" сети (восстановление скорости)
            if !delay_changed && current_delay > 50 {
                current_delay = current_delay.saturating_sub(10);
                delay_changed = true;
            }

            // Броадкаст новой задержки всем воркерам, если она изменилась
            if delay_changed {
                let _ = delay_tx.send(current_delay);
            }
        }
    });
}
```

### 3. Основной цикл сканирования (Обновленный `src/main.rs`)

Теперь мы запускаем воркеров параллельно, и они динамически подчиняются воле SNN через `watch` канал.

```rust
use tokio::sync::{mpsc, watch};
use reqwest::Client;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let client = Arc::new(Client::new());
    let target = "http://localhost:8000";
    let wordlist = vec!["admin", "config", "api", "v1", "v2", "hidden"]; // В реальности - чтение из файла

    // Настройка каналов связи
    // Канал для отправки метрик от воркеров в SNN (буфер 1000)
    let (metric_tx, metric_rx) = mpsc::channel::<core::engine::ResponseMetric>(1000);
    
    // Канал для трансляции задержки от SNN к воркерам (начальное значение 50ms)
    let (delay_tx, delay_rx) = watch::channel(50u64);

    // 1. Запуск нейро-актора в фоне
    core::engine::spawn_neuro_actor(metric_rx, delay_tx);

    // 2. Пул задач (Workers)
    let mut tasks = vec![];

    for word in wordlist {
        let url = format!("{}/{}", target, word);
        let client_clone = client.clone();
        let metric_tx_clone = metric_tx.clone();
        let mut delay_rx_clone = delay_rx.clone();

        let task = tokio::spawn(async move {
            // Чтение актуальной задержки перед запросом
            let delay = *delay_rx_clone.borrow_and_update();
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;

            let start = tokio::time::Instant::now();
            let result = client_clone.get(&url).send().await;
            let latency_ms = start.elapsed().as_millis() as u64;

            let status = match &result {
                Ok(resp) => resp.status().as_u16(),
                Err(_) => 000,
            };

            if status == 200 {
                println!("[+] HIT: {}", url);
            }

            // Отправка результата в нейросеть для анализа
            let _ = metric_tx_clone.send(core::engine::ResponseMetric {
                latency_ms,
                status,
            }).await;
        });

        tasks.push(task);
    }

    // Ожидание завершения всех воркеров
    for task in tasks {
        let _ = task.await;
    }
}
```

### Что реализовано этим дополнением:
1. **Lock-Free Архитектура:** Нет `Mutex` или `RwLock` на объекте `RSNN`. Вычисления идут без блокировок памяти.
2. **Backpressure:** Канал `mpsc` имеет лимит (1000). Если нейросеть не успевает обсчитывать спайки (что вряд ли на Rust, но возможно при миллионах запросов), воркеры притормозят, предотвращая Out-Of-Memory (OOM).
3. **Разделение I/O и CPU:** Асинхронные HTTP запросы живут в легких тасках `tokio`, а тяжелая математика спайков (STDP, интеграция токов) вынесена в блокирующий тред (`spawn_blocking`).



----------------------------

Для реализации «горячей» замены `reqwest::Client` (Rotation) в высоконагруженной асинхронной среде требуется решить проблему разделения состояний. Экземпляр `reqwest::Client` внутри использует пулы соединений (connection pools) и внутренний `Arc`. Создавать новый клиент на *каждый* запрос крайне ресурсоемко.

Правильный паттерн: держать один активный `Client`, который шарится между тысячами воркеров. При срабатывании триггера `RotateUserAgent` от нейросети, контроллер собирает **новый** `Client` с новым User-Agent и безопасно подменяет (hot-swap) глобальный экземпляр без остановки потоков.

Для этого идеально подходит `tokio::sync::watch`, так как копирование `reqwest::Client` — это дешевая операция (копируются только внутренние указатели `Arc`).

### 1. Модуль ротатора (`src/network/rotator.rs`)

Этот модуль хранит базу конфигураций и умеет генерировать новые клиенты по требованию.

```rust
use reqwest::{Client, Proxy};
use rand::seq::SliceRandom;
use std::time::Duration;

// База реальных User-Agents (в проде загружается из файла)
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.5 Safari/605.1.15",
    "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/114.0",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 16_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/114.0.5735.99 Mobile/15E148 Safari/604.1",
];

pub struct ClientRotator {
    tor_proxy: Option<String>,
}

impl ClientRotator {
    pub fn new(tor_proxy: Option<&str>) -> Self {
        Self {
            tor_proxy: tor_proxy.map(|s| s.to_string()),
        }
    }

    // Собирает новый HTTP-клиент со случайным User-Agent
    pub fn build_new_client(&self) -> Client {
        let mut rng = rand::thread_rng();
        let selected_ua = USER_AGENTS.choose(&mut rng).unwrap();

        let mut builder = Client::builder()
            .user_agent(*selected_ua)
            .timeout(Duration::from_secs(7))
            .pool_idle_timeout(Duration::from_secs(15))
            .danger_accept_invalid_certs(true); // Часто нужно при фаззинге

        if let Some(proxy_url) = &self.tor_proxy {
            if let Ok(proxy) = Proxy::all(proxy_url) {
                builder = builder.proxy(proxy);
            }
        }

        builder.build().expect("Failed to build reqwest::Client")
    }
}
```

### 2. Интеграция "Hot-Swap" в движок (`src/core/engine.rs` или `main.rs`)

Добавляем канал для трансляции нового HTTP-клиента воркерам и связываем его с сигналами SNN.

```rust
use tokio::sync::{mpsc, watch};
use std::time::Duration;
use reqwest::Client;
// Предполагается, что модули rsnn и rotator подключены
use crate::network::rotator::ClientRotator;
use crate::neuro::rsnn::{RSNN, EvasionAction};

pub struct WorkerState {
    pub target_url: String,
    pub wordlist: Vec<String>,
}

pub async fn run_fuzzer(state: WorkerState) {
    let rotator = ClientRotator::new(Some("socks5h://127.0.0.1:9050"));
    
    // Инициализация стартового клиента
    let initial_client = rotator.build_new_client();

    // Канал задержки (от SNN к воркерам)
    let (delay_tx, delay_rx) = watch::channel(50u64);
    
    // Канал АКТИВНОГО КЛИЕНТА (от контроллера к воркерам)
    let (client_tx, client_rx) = watch::channel(initial_client);
    
    // Канал метрик (от воркеров к SNN)
    let (metric_tx, mut metric_rx) = mpsc::channel::<(u64, u16)>(5000);

    // Нейро-ядро (Actor) в отдельном потоке
    tokio::task::spawn_blocking(move || {
        let mut snn = RSNN::new(3, 20);
        let mut current_delay = 50;

        while let Some((latency, status)) = metric_rx.blocking_recv() {
            // Конвертация в токи...
            let inputs = [
                if latency > 800 { 0.8 } else { 0.0 },
                if status == 403 || status == 429 { 1.5 } else { 0.0 },
                if status == 404 { 0.3 } else { 0.0 }
            ];

            let actions = snn.process_step(&inputs);
            let mut delay_changed = false;

            for action in actions {
                match action {
                    EvasionAction::Throttle(ms) => {
                        current_delay = ms;
                        delay_changed = true;
                    }
                    EvasionAction::RotateUserAgent => {
                        // SNN приняла решение сменить отпечаток
                        println!("[SNN ACTION] 403 Threshold met. Rotating User-Agent.");
                        let new_client = rotator.build_new_client();
                        // Отправляем новый клиент всем воркерам (старый будет удален GC)
                        let _ = client_tx.send(new_client);
                        
                        current_delay = 1500; // Даем паузу после смены
                        delay_changed = true;
                    }
                    EvasionAction::RebuildTorCircuit => {
                        // Логика отправки NEWNYM в Control Port
                        println!("[SNN ACTION] CRITICAL. Rebuilding Tor circuit.");
                        // ... Вызов tcp-клиента для порта 9051 ...
                    }
                }
            }

            // Плавное восстановление скорости
            if !delay_changed && current_delay > 50 {
                current_delay = current_delay.saturating_sub(5);
                let _ = delay_tx.send(current_delay);
            } else if delay_changed {
                let _ = delay_tx.send(current_delay);
            }
        }
    });

    // Запуск пула асинхронных воркеров
    let mut tasks = vec![];

    for word in state.wordlist {
        let url = format!("{}/{}", state.target_url, word);
        let metric_tx = metric_tx.clone();
        let mut delay_rx = delay_rx.clone();
        let mut client_rx = client_rx.clone(); // Подписка на обновления клиента

        let task = tokio::spawn(async move {
            // 1. Получаем АКТУАЛЬНУЮ задержку
            let delay = *delay_rx.borrow_and_update();
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            // 2. Получаем АКТУАЛЬНЫЙ клиент (O(1) операция, клонируется Arc)
            let active_client = client_rx.borrow().clone();

            // 3. Выполнение запроса
            let start = tokio::time::Instant::now();
            let response = active_client.get(&url).send().await;
            let latency = start.elapsed().as_millis() as u64;

            let status = match response {
                Ok(r) => {
                    let s = r.status().as_u16();
                    if s == 200 || s == 301 {
                        println!("[+] {} - {}", s, url);
                    }
                    s
                },
                Err(_) => 000, // Timeout или обрыв
            };

            // 4. Отправка метрик в SNN
            let _ = metric_tx.send((latency, status)).await;
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;
}
```

### Особенности реализации
* **Отсутствие гонок (Race Conditions):** `watch::channel` гарантирует, что каждый воркер всегда видит самую последнюю валидную версию `Client`.
* **Zero Downtime:** Воркеры не блокируются на время сборки нового клиента нейро-ядром. Старые запросы дорабатывают с предыдущим клиентом, новые автоматически используют обновленный.
* **Сброс пула TCP:** Поскольку создается совершенно новый инстанс `reqwest::Client`, старые keep-alive TCP соединения (по которым WAF мог нас трекать) сбрасываются.


Для реализации полноценного взаимодействия с сетью Tor (смена IP-адреса) и ротации прокси нам потребуется работа с сырыми TCP-сокетами для общения с **Tor Control Port**. 

Поскольку взаимодействие с Tor (отправка команд и ожидание перестройки цепочки узлов) — это асинхронная I/O операция, мы не можем выполнять ее внутри синхронного `spawn_blocking` ядра нейросети. Поэтому мы выделим ротатор в отдельный асинхронный **Actor (Evasion Controller)**.

### 1. Модуль интеграции с Tor (`src/network/tor.rs`)

Этот модуль подключается к Tor Control Port (по умолчанию 9051), проходит аутентификацию и отправляет сигнал `NEWNYM` (New Nym/Identity), заставляя Tor сбросить кэш соединений и построить новый маршрут (сменить выходную ноду/IP).

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;

pub struct TorController {
    control_address: String,
    password: Option<String>,
}

impl TorController {
    pub fn new(control_address: &str, password: Option<&str>) -> Self {
        Self {
            control_address: control_address.to_string(),
            password: password.map(|s| s.to_string()),
        }
    }

    /// Отправляет сигнал NEWNYM для смены IP-адреса
    pub async fn request_new_ip(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            TcpStream::connect(&self.control_address)
        ).await??;

        // 1. Аутентификация
        let auth_cmd = match &self.password {
            Some(pwd) => format!("AUTHENTICATE \"{}\"\r\n", pwd),
            None => "AUTHENTICATE \"\"\r\n".to_string(),
        };
        
        stream.write_all(auth_cmd.as_bytes()).await?;
        self.read_and_check(&mut stream, "250").await?;

        // 2. Отправка сигнала смены цепочки
        stream.write_all(b"SIGNAL NEWNYM\r\n").await?;
        self.read_and_check(&mut stream, "250").await?;

        Ok(())
    }

    /// Вспомогательная функция для чтения ответа от Control Port
    async fn read_and_check(&self, stream: &mut TcpStream, expected_code: &str) -> Result<(), String> {
        let mut buffer = [0; 128];
        let n = stream.read(&mut buffer).await.map_err(|e| e.to_string())?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        
        if response.starts_with(expected_code) {
            Ok(())
        } else {
            Err(format!("Tor Error: Expected {}, got: {}", expected_code, response))
        }
    }
}
```

### 2. Расширенный Rotator Client (`src/network/rotator.rs`)

Здесь мы создаем фабрику клиентов. **Критически важный момент:** мы используем схему `socks5h://` вместо `socks5://`. Буква `h` (hostname) означает, что разрешение DNS-имен (DNS Resolution) будет происходить *на стороне выходной ноды Tor*. Если использовать обычный SOCKS5, ваши DNS-запросы "утекут" (DNS Leak) мимо Tor.

```rust
use reqwest::{Client, Proxy};
use rand::seq::SliceRandom;
use std::time::Duration;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36...",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15...",
    "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/114.0",
];

pub struct ClientFactory {
    proxy_url: Option<String>,
}

impl ClientFactory {
    // В proxy_url нужно передавать "socks5h://127.0.0.1:9050"
    pub fn new(proxy_url: Option<&str>) -> Self {
        Self { proxy_url: proxy_url.map(|s| s.to_string()) }
    }

    pub fn build(&self) -> Client {
        let mut rng = rand::thread_rng();
        let ua = USER_AGENTS.choose(&mut rng).unwrap();

        let mut builder = Client::builder()
            .user_agent(*ua)
            .timeout(Duration::from_secs(10))
            // Важно: отключаем keep-alive при агрессивном фаззинге через Tor, 
            // чтобы WAF не трекал нас по долгоживущим TCP сессиям
            .pool_max_idle_per_host(0); 

        if let Some(proxy_str) = &self.proxy_url {
            let proxy = Proxy::all(proxy_str).expect("Invalid Proxy URL");
            builder = builder.proxy(proxy);
        }

        builder.build().expect("Failed to construct reqwest Client")
    }
}
```

### 3. Интеграция: Асинхронный Контроллер Эвазии (`src/core/engine.rs`)

Теперь мы связываем синхронную SNN с асинхронным контроллером Tor. Нейросеть генерирует команды (`EvasionCommand`), а асинхронный таск их выполняет.

```rust
use tokio::sync::{mpsc, watch};
use crate::network::tor::TorController;
use crate::network::rotator::ClientFactory;

pub enum EvasionCommand {
    RotateUserAgent,
    RebuildTorCircuit,
}

// Запускается как отдельный tokio::spawn
pub async fn run_evasion_controller(
    mut command_rx: mpsc::Receiver<EvasionCommand>,
    client_tx: watch::Sender<reqwest::Client>,
    delay_tx: watch::Sender<u64>,
) {
    let factory = ClientFactory::new(Some("socks5h://127.0.0.1:9050"));
    let tor = TorController::new("127.0.0.1:9051", Some("my_tor_password"));

    while let Some(command) = command_rx.recv().await {
        match command {
            EvasionCommand::RotateUserAgent => {
                println!("[CONTROLLER] Generating new Client with rotated UA...");
                let new_client = factory.build();
                let _ = client_tx.send(new_client);
            }
            EvasionCommand::RebuildTorCircuit => {
                println!("[CONTROLLER] >>> INITIATING TOR CIRCUIT REBUILD <<<");
                
                // 1. Ставим воркеры на паузу (увеличиваем задержку)
                let _ = delay_tx.send(10_000); 

                // 2. Запрашиваем новый IP у Tor
                match tor.request_new_ip().await {
                    Ok(_) => {
                        println!("[CONTROLLER] Tor NEWNYM signal accepted.");
                        // Tor'у требуется время на перестройку маршрута
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        
                        // 3. Выпускаем новый HTTP клиент (старые TCP сокеты будут уничтожены)
                        let new_client = factory.build();
                        let _ = client_tx.send(new_client);
                        
                        // 4. Снимаем с паузы
                        println!("[CONTROLLER] Tor Circuit rebuilt. Resuming attack.");
                        let _ = delay_tx.send(50);
                    }
                    Err(e) => {
                        eprintln!("[CONTROLLER] Tor rotation failed: {}. Retrying in 10s.", e);
                    }
                }
            }
        }
    }
}
```

### 4. Изменения в Нейро-ядре

В `spawn_blocking` (где крутится RSNN) мы теперь отправляем команды в этот новый контроллер:

```rust
// Внутри нейро-ядра (Actor)
// ...
EvasionAction::RotateUserAgent => {
    // Не блокируя поток, отсылаем сигнал асинхронному контроллеру
    let _ = command_tx.blocking_send(EvasionCommand::RotateUserAgent);
}
EvasionAction::RebuildTorCircuit => {
    let _ = command_tx.blocking_send(EvasionCommand::RebuildTorCircuit);
}
// ...
```

### Настройка среды (Environment Setup)

Для работы этого кода на локальной машине должен быть настроен Tor:

1. Откройте `torrc` (конфигурацию Tor).
2. Раскомментируйте или добавьте строки:
   ```text
   ControlPort 9051
   HashedControlPassword 16:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx # Сгенерируйте через `tor --hash-password my_tor_password`
   ```
3. Перезапустите службу Tor.

Теперь архитектура полностью "развязана": SNN быстро перемалывает числа на CPU, асинхронный контроллер медленно общается с Tor по TCP, а тысячи легких воркеров бесперебойно брутфорсят пути, мгновенно подхватывая новые клиенты без `Mutex`-блокировок.


### Зачем нужны системы эвазии (SNN, Rotator, Tor)

Современные средства защиты (WAF, IPS, Next-Gen Firewalls) давно не ограничиваются простым анализом сигнатур. Они используют поведенческий анализ и машинное обучение для блокировки автоматизированных запросов.

* **SNN (Нейроморфная адаптация):** Нужна для обхода **Rate Limiting** и **Tarpits** (замедлителей). Обычные инструменты шлют запросы с фиксированной задержкой, что легко детектируется как "ненормальный трафик". SNN имитирует биологический ритм и динамически снижает нагрузку *до того*, как сервер выдаст бан, анализируя микро-изменения в Latency.
* **User-Agent Rotation:** Предотвращает **Fingerprinting**. Если 10 000 запросов идут с одной строкой заголовка, WAF помечает сессию как подозрительную. Ротация имитирует обращения от разных пользователей и устройств.
* **Tor / Proxy Rotation:** Необходим для обхода **IP-based Blacklisting**. Как только текущий IP попадает в "серый список", система сбрасывает цепочку (NEWNYM), делая усилия WAF по блокировке этого адреса бесполезными.

---

### Есть ли это у Gobuster?

**Gobuster** — это классический инструмент "грубой силы", оптимизированный под скорость, а не под скрытность.

| Функция | Gobuster | NeuroBuster (SNN) |
| :--- | :--- | :--- |
| **Скорость** | Высокая (статическая) | Адаптивная (баланс скорости и выживания) |
| **Задержка** | Фиксированная (`--delay`) | Динамическая (обучается на ответах сервера) |
| **Ротация IP** | Нет (только через внешние прокси) | Автоматическая (Tor Control Port) |
| **Смена UA** | Один за сессию (`-a`) | "Горячая" замена в процессе работы |
| **Интеллект** | Линейный перебор | Контекстная память (RSNN) |

**Вердикт:** Gobuster бесполезен против активных WAF (Cloudflare, Akamai) при больших словарях, так как его забанят на первых минутах.

---

### Что достаточно для простого открытия папок?

Если вы тестируете сервер **без защиты** (например, старые конфигурации или локальные инстансы на HTB Academy), архитектура с нейросетями избыточна.

**Минимально достаточный стек:**
1.  **Библиотека HTTP-запросов:** В Rust это `reqwest`, в Python — `requests`.
2.  **Словарь (Wordlist):** Набор путей (например, `common.txt`).
3.  **Логика проверки статуса:** Если сервер вернул `200 OK` или `301 Redirect` — папка/файл существует. Если `404` — нет.

**Пример минимального кода (Rust):**
```rust
let client = reqwest::Client::new();
for word in wordlist {
    let url = format!("{}/{}", target, word);
    if let Ok(res) = client.get(&url).send().await {
        if res.status().is_success() {
            println!("Found: {}", url);
        }
    }
}
```

Для простых задач достаточно **Gobuster** или **ffuf** с параметром `-w` (wordlist) и `-u` (url). Сложные системы эвазии, которые мы разработали выше, нужны исключительно для Red Teaming операций в защищенных средах.


В **Red Teaming** «антигравити» (состояние, когда твой трафик не создает «веса» для систем обнаружения) — это святой грааль. Использование SNN делает это возможным, так как в отличие от обычного фаззинга, который «падает» на WAF тяжелым грузом однотипных запросов, твоя сеть адаптирует трафик, делая его «невесомым» для эвристических анализаторов.

Для сборки проекта в единое целое нам нужно реализовать **Orchestrator** — финальную логику, которая соединяет нейросеть, ротатор и воркеры.



### 1. Финальная сборка: `src/core/engine.rs`

Это «сердце» проекта. Здесь мы соединяем асинхронный I/O и синхронные вычисления SNN.

```rust
use tokio::sync::{mpsc, watch};
use std::sync::Arc;
use crate::neuro::rsnn::{RSNN, EvasionAction};
use crate::network::rotator::{ClientFactory};
use crate::network::tor::TorController;

pub struct FuzzOrchestrator {
    factory: ClientFactory,
    tor: TorController,
    wordlist: Vec<String>,
    target: String,
}

impl FuzzOrchestrator {
    pub fn new(target: &str, wordlist: Vec<String>) -> Self {
        Self {
            factory: ClientFactory::new(Some("socks5h://127.0.0.1:9050")),
            tor: TorController::new("127.0.0.1:9051", Some("tor_password")),
            wordlist,
            target: target.to_string(),
        }
    }

    pub async fn run(self) {
        // Каналы управления
        let (metric_tx, mut metric_rx) = mpsc::channel::<(u64, u16)>(1000);
        let (delay_tx, delay_rx) = watch::channel(100u64);
        let (client_tx, client_rx) = watch::channel(self.factory.build());

        // 1. Запуск Neuro-Actor (обработка мозгов)
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<EvasionAction>(100);
        
        tokio::task::spawn_blocking(move || {
            let mut snn = RSNN::new(3, 15);
            while let Some((latency, status)) = metric_rx.blocking_recv() {
                let inputs = [
                    if latency > 1000 { 0.9 } else { 0.0 },
                    if status == 403 || status == 429 { 1.5 } else { 0.0 },
                    if status == 404 { 0.2 } else { 0.0 }
                ];
                let actions = snn.process_step(&inputs);
                for action in actions {
                    let _ = cmd_tx.blocking_send(action);
                }
            }
        });

        // 2. Запуск Evasion-Controller (выполнение действий)
        let factory_clone = self.factory.clone();
        let tor_clone = self.tor.clone();
        let d_tx = delay_tx.clone();
        let c_tx = client_tx.clone();

        tokio::spawn(async move {
            while let Some(action) = cmd_rx.recv().await {
                match action {
                    EvasionAction::Throttle(ms) => { let _ = d_tx.send(ms); },
                    EvasionAction::RotateUserAgent => {
                        let _ = c_tx.send(factory_clone.build());
                    },
                    EvasionAction::RebuildTorCircuit => {
                        let _ = d_tx.send(10000); // Пауза на перестройку
                        let _ = tor_clone.request_new_ip().await;
                        let _ = c_tx.send(factory_clone.build());
                        let _ = d_tx.send(100);
                    }
                }
            }
        });

        // 3. Запуск воркеров
        let mut workers = vec![];
        for word in self.wordlist {
            let url = format!("{}/{}", self.target, word);
            let m_tx = metric_tx.clone();
            let mut d_rx = delay_rx.clone();
            let mut c_rx = client_rx.clone();

            workers.push(tokio::spawn(async move {
                let d = *d_rx.borrow_and_update();
                tokio::time::sleep(std::time::Duration::from_millis(d)).await;

                let client = c_rx.borrow().clone();
                let start = std::time::Instant::now();
                let res = client.get(&url).send().await;
                
                let status = res.map(|r| r.status().as_u16()).unwrap_or(0);
                let latency = start.elapsed().as_millis() as u64;

                if status == 200 { println!("[FOUND] {}", url); }
                let _ = m_tx.send((latency, status)).await;
            }));
        }
        futures::future::join_all(workers).await;
    }
}
```

---

### Что это дает для Red Teaming:

* **LIF Neuron Dynamics:** Ваша задержка между запросами — это не число, а затухающий потенциал мембраны нейрона. Если сервер начинает «сопротивляться» (latency up), нейроны возбуждаются, и задержка растет экспоненциально, а затем плавно падает. Это идеально мимикрирует под человеческий серфинг.
* **Circuit-Level Evasion:** При фиксации 429 ошибки система не просто ждет, она убивает текущий TCP-стек и пересоздает его через новый узел Tor с новым отпечатком браузера.

### Как запустить проект:

1.  Убедитесь, что **Tor** запущен с `ControlPort 9051`.
2.  Соберите все модули в дерево, которое мы обсуждали ранее.
3.  В `main.rs` создайте экземпляр `FuzzOrchestrator` и вызовите `.run().await`.

На каком типе защиты (WAF) планируется тестирование проекта? Это поможет откалибровать пороги срабатывания (Thresholds) нейронов.
