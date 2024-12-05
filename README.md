# Database Testing Guide for PostgreSQL, MySQL, and SQLite

This README provides a step-by-step guide on how to set up and test PostgreSQL, MySQL, and SQLite databases using Docker and the command line. It covers basic commands to list tables, describe table schema, and run SQL queries.

---

## Table of Contents

1. [PostgreSQL](#postgresql)
   - Accessing PostgreSQL in Docker
   - Running SQL Queries
   - Exiting PostgreSQL Shell
2. [MySQL](#mysql)
   - Accessing MySQL in Docker
   - Running SQL Queries
   - Exiting MySQL Shell
3. [SQLite](#sqlite)
   - Installing SQLite on Manjaro Linux
   - Installing SQLite on macOS
   - Running SQL Queries
   - Exiting SQLite Shell

---

## Commands
```sh
# Non-enterprise
curl http://localhost:5080/api/default/default/_json -i -u "root@example.com:Complexpass#123"  -d "@olympics.json"

# Enterprise
curl http://localhost:5080/api/team1/default/_json -i -u "root@example.com:Complexpass#123"  -d "@output.json"
```


## PostgreSQL

### 1. Accessing PostgreSQL in Docker

If you have PostgreSQL running in a Docker container, you can access the container shell and the PostgreSQL prompt as follows:

1. **List all running Docker containers**:

    ```bash
    docker ps
    ```

2. **Access the PostgreSQL container shell**:

    ```bash
    docker exec -it <container_id_or_name> /bin/bash
    ```

3. **Connect to PostgreSQL**:

    ```bash
    psql -U <your_username> -d <your_database>
    ```

   Replace `<your_username>` and `<your_database>` with the actual username and database name.

### 2. Running SQL Queries

Once you're in the PostgreSQL shell, you can run the following commands:

1. **List all tables**:

    ```sql
    \dt
    ```

2. **Describe a specific table**:

    ```sql
    \d table_name
    ```

3. **Run a SQL query**:

    ```sql
    SELECT * FROM table_name;
    ```

### 3. Exiting PostgreSQL Shell

To exit the PostgreSQL shell, type:

```sql
\q
```

---

## MySQL

### 1. Accessing MySQL in Docker

To access the MySQL shell in a running Docker container:

1. **List all running Docker containers**:

    ```bash
    docker ps
    ```

2. **Access the MySQL container shell**:

    ```bash
    docker exec -it <container_id_or_name> mysql -u<username> -p
    ```

   Replace `<container_id_or_name>` with the actual container ID or name and `<username>` with your MySQL username (usually `root`). After running the command, you'll be prompted for the password.

### 2. Running SQL Queries

Once you're inside the MySQL shell, you can run the following SQL queries:

1. **Show existing databases**:

    ```sql
    SHOW DATABASES;
    ```

2. **Select a database**:

    ```sql
    USE your_database_name;
    ```

3. **Show all tables**:

    ```sql
    SHOW TABLES;
    ```

4. **Describe a table**:

    ```sql
    DESCRIBE table_name;
    ```

5. **Insert a test row**:

    ```sql
    INSERT INTO short_urls (original_url, short_id) VALUES ('https://example.com/very-long-url', 'short123');
    ```

6. **Select all rows from a table**:

    ```sql
    SELECT * FROM short_urls;
    ```

### 3. Exiting MySQL Shell

To exit the MySQL shell, type:

```sql
EXIT;
```

---

## SQLite

### 1. Installing SQLite on Manjaro Linux

If SQLite is not installed on your Manjaro system, you can install it using the following command:

```bash
sudo pacman -S sqlite
```

### 2. Installing SQLite on macOS

If you're using macOS, you can install SQLite using **Homebrew**. Homebrew is a package manager for macOS that makes it easy to install software.

1. **Install Homebrew** (if it’s not already installed):

    Open your terminal and run the following command to install Homebrew:

    ```bash
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    ```

2. **Install SQLite using Homebrew**:

    Once Homebrew is installed, you can install SQLite by running:

    ```bash
    brew install sqlite
    ```

3. **Verify the installation**:

    After installing, verify that SQLite is installed correctly by checking its version:

    ```bash
    sqlite3 --version
    ```

    You should see output similar to this (version may vary):

    ```
    3.41.2 2023-03-10 12:34:56 ...
    ```

### 3. Running SQL Queries

Once SQLite is installed, you can open an SQLite database and run queries.

1. **Open the SQLite database**:

    ```bash
    sqlite3 /path/to/your/database.db
    ```

   Replace `/path/to/your/database.db` with the actual path to your SQLite database file.

2. **List all tables**:

    ```sql
    .tables
    ```

3. **View the schema of a table**:

    ```sql
    .schema table_name
    ```

4. **Query data from a table**:

    ```sql
    SELECT * FROM table_name LIMIT 10;
    ```

5. **Insert a row into a table**:

    ```sql
    INSERT INTO short_urls (original_url, short_id) VALUES ('https://example.com/very-long-url', 'short123');
    ```

6. **Count the number of rows in a table**:

    ```sql
    SELECT COUNT(*) FROM table_name;
    ```

### 4. Exiting SQLite Shell

To exit the SQLite shell, type:

```sql
.exit
```

---

## Common Commands Across All Databases

- **List all tables**:
  - PostgreSQL: `\dt`
  - MySQL: `SHOW TABLES;`
  - SQLite: `.tables`

- **Describe a table's schema**:
  - PostgreSQL: `\d table_name`
  - MySQL: `DESCRIBE table_name;`
  - SQLite: `.schema table_name`

- **Run a SQL query**:
  - PostgreSQL, MySQL, SQLite: `SELECT * FROM table_name;`

- **Exit the database shell**:
  - PostgreSQL: `\q`
  - MySQL: `EXIT;`
  - SQLite: `.exit`

---

## Troubleshooting

- **Container not running**: Make sure the Docker container is running by checking with `docker ps`.
- **Invalid credentials**: Double-check the username and password when connecting to MySQL or PostgreSQL.
- **Database file not found**: Ensure the correct path is provided when opening an SQLite database file.
- **No data returned**: Verify that the table contains data and that your query conditions are correct.

---

By following this guide, you should be able to successfully connect to and test PostgreSQL, MySQL, and SQLite databases using Docker and the command line. Happy querying!
