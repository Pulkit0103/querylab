-- Sample "target" database for trying QueryLab locally.
-- Executed automatically by the sample-db container on first start.

CREATE SCHEMA IF NOT EXISTS sales;

CREATE TABLE public.departments (
    id          serial PRIMARY KEY,
    name        text NOT NULL UNIQUE,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE public.employees (
    id             serial PRIMARY KEY,
    department_id  integer NOT NULL REFERENCES public.departments (id),
    manager_id     integer REFERENCES public.employees (id),
    first_name     text NOT NULL,
    last_name      text NOT NULL,
    email          text NOT NULL UNIQUE,
    salary         numeric(12, 2) NOT NULL CHECK (salary > 0),
    hired_on       date NOT NULL,
    is_active      boolean NOT NULL DEFAULT true,
    metadata       jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX employees_department_idx ON public.employees (department_id);

CREATE TABLE sales.customers (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    country     char(2) NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sales.orders (
    id            bigserial PRIMARY KEY,
    customer_id   uuid NOT NULL REFERENCES sales.customers (id) ON DELETE CASCADE,
    sales_rep_id  integer REFERENCES public.employees (id),
    status        text NOT NULL CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled')),
    total         numeric(12, 2) NOT NULL,
    ordered_at    timestamptz NOT NULL DEFAULT now(),
    tags          text[] NOT NULL DEFAULT '{}'
);
CREATE INDEX orders_customer_idx ON sales.orders (customer_id);
CREATE INDEX orders_ordered_at_idx ON sales.orders (ordered_at);

INSERT INTO public.departments (name)
VALUES ('Engineering'), ('Sales'), ('Marketing'), ('Finance'), ('Support');

INSERT INTO public.employees (department_id, manager_id, first_name, last_name, email, salary, hired_on, is_active, metadata)
SELECT
    1 + (g % 5),
    CASE WHEN g > 5 THEN 1 + (g % 5) END,
    (ARRAY['Ada','Grace','Linus','Margaret','Ken','Barbara','Dennis','Frances','Edsger','Radia'])[1 + g % 10],
    (ARRAY['Lovelace','Hopper','Torvalds','Hamilton','Thompson','Liskov','Ritchie','Allen','Dijkstra','Perlman'])[1 + (g / 10) % 10],
    'employee' || g || '@example.com',
    round((55000 + random() * 95000)::numeric, 2),
    date '2015-01-01' + (g * 37 % 3500),
    g % 11 <> 0,
    jsonb_build_object('level', 1 + g % 4, 'remote', g % 3 = 0)
FROM generate_series(1, 200) AS g;

INSERT INTO sales.customers (name, country)
SELECT 'Customer ' || g, (ARRAY['US','GB','DE','IN','FR','JP','BR','CA'])[1 + g % 8]
FROM generate_series(1, 500) AS g;

INSERT INTO sales.orders (customer_id, sales_rep_id, status, total, ordered_at, tags)
SELECT
    c.id,
    1 + (random() * 199)::int,
    (ARRAY['pending','paid','shipped','cancelled'])[1 + (random() * 3)::int],
    round((10 + random() * 990)::numeric, 2),
    now() - (random() * interval '365 days'),
    CASE WHEN random() < 0.3 THEN ARRAY['priority'] ELSE '{}' END
FROM sales.customers c, generate_series(1, 20);

CREATE VIEW sales.revenue_by_country AS
SELECT c.country, count(*) AS orders, sum(o.total) AS revenue
FROM sales.orders o
JOIN sales.customers c ON c.id = o.customer_id
WHERE o.status <> 'cancelled'
GROUP BY c.country;

ANALYZE public.departments, public.employees, sales.customers, sales.orders;

-- Least-privilege role intended for QueryLab connections.
-- (Requires the script runner to have CREATEROLE, e.g. the container superuser.)
CREATE ROLE querylab_reader LOGIN PASSWORD 'reader_password';
ALTER ROLE querylab_reader SET default_transaction_read_only = on;
GRANT CONNECT ON DATABASE sampledb TO querylab_reader;
GRANT USAGE ON SCHEMA public, sales TO querylab_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public, sales TO querylab_reader;
ALTER DEFAULT PRIVILEGES IN SCHEMA public, sales GRANT SELECT ON TABLES TO querylab_reader;
